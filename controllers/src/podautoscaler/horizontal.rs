use std::{
    cmp::{max, min, Ordering},
    collections::{HashMap, HashSet, LinkedList},
};

use anyhow::{Context, Error, Result};
use chrono::{Duration, Local, NaiveDateTime};
use futures_delay_queue::{delay_queue, DelayQueue};
use futures_intrusive::{buffer::GrowingHeapBuf, channel::shared::GenericReceiver};
use parking_lot::RawMutex;
use resources::{
    informer::{EventHandler, Informer, ResyncHandler, Store},
    objects::{
        hpa::{
            FunctionMetricSource, HPAScalingRules, HorizontalPodAutoscaler,
            HorizontalPodAutoscalerBehavior, HorizontalPodAutoscalerStatus, MetricSource,
            MetricTarget, PolicySelection, ResourceMetricSource, ScalingPolicyType,
        },
        pod::Pod,
        KubeObject, Labels, Object,
    },
};
use tokio::{
    select,
    sync::{
        mpsc,
        mpsc::{Receiver, Sender},
    },
    task::JoinHandle,
};

use crate::{
    replica_calculator::ReplicaCalculator,
    utils::{create_lister_watcher, get_scale_target, post_update},
    SYNC_PERIOD,
};

#[derive(Debug, Clone)]
struct Recommendation {
    pub replicas: u32,
    pub time: NaiveDateTime,
}

#[derive(Debug, Clone)]
struct ScaleEvent {
    pub change: u32,
    pub time: NaiveDateTime,
}

#[derive(Debug)]
struct ResyncNotification;

pub struct PodAutoscaler {
    rx: Receiver<String>,
    resync_rx: Receiver<ResyncNotification>,
    hpa_informer: Option<JoinHandle<Result<(), Error>>>,
    hpa_store: Store<HorizontalPodAutoscaler>,
    pod_informer: Option<JoinHandle<Result<(), Error>>>,

    calculator: ReplicaCalculator,
    /// Desired replicas recommendations
    recommendations: HashMap<String, LinkedList<Recommendation>>,
    scale_up_events: HashMap<String, LinkedList<ScaleEvent>>,
    scale_down_events: HashMap<String, LinkedList<ScaleEvent>>,

    work_queue: DelayQueue<String, GrowingHeapBuf<String>>,
    work_queue_rx: GenericReceiver<RawMutex, String, GrowingHeapBuf<String>>,
    in_queue: HashSet<String>,
}

impl PodAutoscaler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<String>(16);
        let (resync_tx, resync_rx) = mpsc::channel::<ResyncNotification>(16);
        let hpa_informer = PodAutoscaler::create_hpa_informer(tx, resync_tx);
        let hpa_store = hpa_informer.get_store();
        let pod_informer = PodAutoscaler::create_pod_informer();
        let pod_store = pod_informer.get_store();

        let hpa_informer = tokio::spawn(async move { hpa_informer.run().await });
        let pod_informer = tokio::spawn(async move { pod_informer.run().await });

        let (work_queue, work_queue_rx) = delay_queue::<String>();

        Self {
            rx,
            resync_rx,
            hpa_informer: Some(hpa_informer),
            hpa_store,
            pod_informer: Some(pod_informer),

            calculator: ReplicaCalculator::new(pod_store),
            recommendations: HashMap::new(),
            scale_up_events: HashMap::new(),
            scale_down_events: HashMap::new(),

            work_queue,
            work_queue_rx,
            in_queue: HashSet::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Horizontal Pod Autoscaler started");

        loop {
            select! {
                Some(hpa_name) = self.rx.recv() => {
                    // Enqueue immediately if HPA is newly added of updated
                    self.in_queue.insert(hpa_name.to_owned());
                    self.work_queue.insert_at(hpa_name, std::time::Instant::now());
                },
                Some(_) = self.resync_rx.recv() => {
                    self.handle_resync().await;
                },
                Some(hpa_name) = self.work_queue_rx.receive() => {
                    self.in_queue.remove(&hpa_name);
                    let store = self.hpa_store.read().await;
                    let object = store.get(&format!("/api/v1/horizontalpodautoscalers/{}", hpa_name));
                    match object {
                        Some(object) => {
                            let object = object.clone();
                            drop(store);
                            let result = self.reconcile(object).await;
                            match result {
                                Ok(_) => tracing::info!("Reconciled HPA {}", hpa_name),
                                Err(e) =>
                                    tracing::error!("Error reconciling {}: {:#}", hpa_name, e)
                                ,
                            }
                            self.enqueue_hpa(&hpa_name, std::time::Duration::from_secs(SYNC_PERIOD as u64));
                        },
                        None => {
                            tracing::info!("Horizontal Pod Autoscaler {} deleted", hpa_name);
                            self.recommendations.remove(&hpa_name);
                            self.scale_up_events.remove(&hpa_name);
                            self.scale_down_events.remove(&hpa_name);
                        },
                    }
                }
                else => break
            }
        }

        let hpa_informer = std::mem::replace(&mut self.hpa_informer, None);
        hpa_informer.unwrap().await??;
        let pod_informer = std::mem::replace(&mut self.pod_informer, None);
        pod_informer.unwrap().await??;
        tracing::info!("Horizontal Pod Autoscaler exited");
        Ok(())
    }

    async fn reconcile(&mut self, mut hpa: HorizontalPodAutoscaler) -> Result<()> {
        let now = Local::now().naive_utc();
        let hpa_name = &hpa.metadata.name;
        let mut target = get_scale_target(&hpa.spec.scale_target_ref).await?;
        let status = hpa
            .status
            .as_ref()
            .with_context(|| "Failed to get HPA status")?;

        match target {
            KubeObject::ReplicaSet(mut rs) => {
                let current_replicas = rs.spec.replicas;
                // Initialize recommendations when needed
                if !self.recommendations.contains_key(hpa_name) {
                    self.recommendations
                        .entry(hpa_name.to_owned())
                        .or_insert_with(LinkedList::new)
                        .push_back(Recommendation {
                            replicas: current_replicas,
                            time: now,
                        });
                }

                // ReplicaSet might have been modified by serverless router
                if current_replicas != status.desired_replicas {
                    self.recommendations
                        .entry(hpa_name.to_owned())
                        .or_insert_with(LinkedList::new)
                        .push_back(Recommendation {
                            replicas: current_replicas,
                            time: now,
                        });
                }

                let desired_replicas = if current_replicas == 0 {
                    // Scaling disabled
                    tracing::info!("Scaling disabled for {}", hpa_name);
                    0
                } else if current_replicas > hpa.spec.max_replicas {
                    hpa.spec.max_replicas
                } else if current_replicas < hpa.spec.min_replicas {
                    hpa.spec.min_replicas
                } else {
                    let desired_replicas = match &hpa.spec.metrics {
                        MetricSource::Resource(metrics) => {
                            self.compute_resource_replicas(
                                metrics,
                                current_replicas,
                                &rs.spec.selector,
                            )
                            .await?
                        },
                        MetricSource::Function(metrics) => {
                            self.compute_function_replicas(metrics, current_replicas)
                                .await?
                        },
                    };
                    tracing::info!("Desired replicas for {}: {}", hpa_name, desired_replicas);
                    self.normalize_desired_replicas(
                        hpa_name,
                        &hpa,
                        current_replicas,
                        desired_replicas,
                    )
                };

                if current_replicas != desired_replicas {
                    // Do scale
                    rs.spec.replicas = desired_replicas;
                    target = KubeObject::ReplicaSet(rs);
                    post_update(&target).await?;
                    tracing::info!(
                        "Scaled {} from {} to {}",
                        target.name(),
                        current_replicas,
                        desired_replicas
                    );
                    self.record_scale_event(
                        hpa_name,
                        &hpa.spec.behavior,
                        current_replicas,
                        desired_replicas,
                    );
                }
                let new_status = HorizontalPodAutoscalerStatus {
                    current_replicas: if desired_replicas == 0 {
                        0
                    } else {
                        current_replicas
                    },
                    desired_replicas,
                    last_scale_time: if current_replicas != desired_replicas {
                        Some(now)
                    } else {
                        status.last_scale_time
                    },
                };
                if !status.eq(&new_status) {
                    // Update and post status
                    hpa.status = Some(new_status);
                    post_update(&KubeObject::HorizontalPodAutoscaler(hpa)).await?;
                }
            },
            _ => {
                tracing::warn!("{} is not scalable", target.kind());
            },
        }
        Ok(())
    }

    async fn compute_resource_replicas(
        &self,
        metrics: &ResourceMetricSource,
        current_replicas: u32,
        selector: &Labels,
    ) -> Result<u32, Error> {
        match metrics.target {
            MetricTarget::AverageUtilization(target_utilization) => {
                self.calculator
                    .calc_replicas_by_utilization(
                        current_replicas,
                        target_utilization,
                        &metrics.name,
                        selector,
                    )
                    .await
            },
            MetricTarget::AverageValue(target_value) => {
                self.calculator
                    .calc_replicas_by_value(current_replicas, target_value, &metrics.name, selector)
                    .await
            },
        }
    }

    async fn compute_function_replicas(
        &self,
        metrics: &FunctionMetricSource,
        current_replicas: u32,
    ) -> Result<u32, Error> {
        self.calculator
            .calc_function_replicas(current_replicas, metrics.target, &metrics.name)
            .await
    }

    fn normalize_desired_replicas(
        &mut self,
        hpa_name: &String,
        hpa: &HorizontalPodAutoscaler,
        current_replicas: u32,
        mut desired_replicas: u32,
    ) -> u32 {
        let stabilized = self.stabilize_recommendation(
            hpa_name,
            current_replicas,
            desired_replicas,
            &hpa.spec.behavior,
        );
        if stabilized != desired_replicas {
            tracing::info!(
                "Stabilized {} from {} to {}",
                hpa_name,
                desired_replicas,
                stabilized
            );
        }
        desired_replicas = self.conform_with_behavior(hpa_name, hpa, current_replicas, stabilized);
        if desired_replicas != stabilized {
            tracing::info!(
                "Conformed {} from {} to {}",
                hpa_name,
                stabilized,
                desired_replicas
            );
        }
        max(hpa.spec.min_replicas, desired_replicas)
    }

    /// Stabilize recommendation with stabilization window specified in bevavior.
    fn stabilize_recommendation(
        &mut self,
        hpa_name: &String,
        current_replicas: u32,
        desired_replicas: u32,
        behavior: &HorizontalPodAutoscalerBehavior,
    ) -> u32 {
        let now = Local::now().naive_utc();
        let mut recommendation = current_replicas;
        let mut up_recommendation = desired_replicas;
        let mut down_recommendation = desired_replicas;
        let up_cutoff =
            now - Duration::seconds(behavior.scale_up.stabilization_window_seconds.into());
        let down_cutoff =
            now - Duration::seconds(behavior.scale_down.stabilization_window_seconds.into());

        let recommendations = self.recommendations.get_mut(hpa_name);
        if let Some(recommendations) = recommendations {
            // Remove outdated recommendations
            while recommendations
                .front()
                .map_or(false, |rec| rec.time < up_cutoff && rec.time < down_cutoff)
            {
                tracing::debug!(
                    "Removed outdated recommendation {:?}",
                    recommendations.front()
                );
                recommendations.pop_front();
            }
            // Find upper and lower bounds
            for rec in recommendations.iter() {
                if rec.time >= up_cutoff {
                    up_recommendation = up_recommendation.min(rec.replicas);
                }
                if rec.time >= down_cutoff {
                    down_recommendation = down_recommendation.max(rec.replicas);
                }
            }
            // Stabilize
            recommendation = recommendation
                .max(up_recommendation)
                .min(down_recommendation);
            tracing::debug!(
                "up_recommendation: {}, down_recommendation: {}",
                up_recommendation,
                down_recommendation
            );
            // Record **unstabilized** recommendation
            recommendations.push_back(Recommendation {
                replicas: desired_replicas,
                time: now,
            });
        }
        recommendation
    }

    /// Normalize desired replicas count with the rate defined in scaling behavior.
    fn conform_with_behavior(
        &self,
        hpa_name: &String,
        hpa: &HorizontalPodAutoscaler,
        current_replicas: u32,
        desired_replicas: u32,
    ) -> u32 {
        match desired_replicas.cmp(&current_replicas) {
            // Scaling up
            Ordering::Greater => {
                let scale_up_limit = self
                    .calc_scale_up_limit(
                        current_replicas,
                        self.scale_up_events.get(hpa_name),
                        &hpa.spec.behavior.scale_up,
                    )
                    .max(current_replicas);
                let mut max_replicas = hpa.spec.max_replicas;
                if max_replicas > scale_up_limit {
                    max_replicas = scale_up_limit;
                    tracing::info!("Scale up limited to {} for {}", max_replicas, hpa_name);
                }
                desired_replicas.min(max_replicas)
            },
            // Scaling down
            Ordering::Less => {
                let scale_down_limit = self
                    .calc_scale_down_limit(
                        current_replicas,
                        self.scale_down_events.get(hpa_name),
                        &hpa.spec.behavior.scale_down,
                    )
                    .min(current_replicas);
                let mut min_replicas = hpa.spec.min_replicas;
                if min_replicas < scale_down_limit {
                    min_replicas = scale_down_limit;
                    tracing::info!("Scale down limited to {} for {}", min_replicas, hpa_name);
                }
                desired_replicas.max(min_replicas)
            },
            Ordering::Equal => desired_replicas,
        }
    }

    fn calc_scale_up_limit(
        &self,
        current_replicas: u32,
        events: Option<&LinkedList<ScaleEvent>>,
        rules: &HPAScalingRules,
    ) -> u32 {
        if rules.select_policy == PolicySelection::Disabled {
            return current_replicas;
        }
        let (mut result, select_fn): (u32, fn(u32, u32) -> u32) = match rules.select_policy {
            PolicySelection::Min => (u32::MAX, min),
            PolicySelection::Max => (u32::MIN, max),
            PolicySelection::Disabled => unreachable!(),
        };
        for policy in &rules.policies {
            let replicas_added_in_current_period =
                self.get_replicas_change_in_period(policy.period_seconds, events);
            tracing::debug!(
                "Replicas added in current period: {}",
                replicas_added_in_current_period
            );
            let period_start = current_replicas - replicas_added_in_current_period;
            let period_limit = match policy.type_ {
                ScalingPolicyType::Pods => period_start + policy.value,
                ScalingPolicyType::Percent => {
                    (period_start as f64 * (1.0 + policy.value as f64 / 100.0)).ceil() as u32
                },
            };
            result = select_fn(result, period_limit);
        }
        result
    }

    fn calc_scale_down_limit(
        &self,
        current_replicas: u32,
        events: Option<&LinkedList<ScaleEvent>>,
        rules: &HPAScalingRules,
    ) -> u32 {
        if rules.select_policy == PolicySelection::Disabled {
            return current_replicas;
        }
        let (mut result, select_fn): (u32, fn(u32, u32) -> u32) = match rules.select_policy {
            // Minimum change results in maximum value
            PolicySelection::Min => (u32::MIN, max),
            PolicySelection::Max => (u32::MAX, min),
            PolicySelection::Disabled => unreachable!(),
        };
        for policy in &rules.policies {
            let replicas_deleted_in_current_period =
                self.get_replicas_change_in_period(policy.period_seconds, events);
            tracing::debug!(
                "Replicas deleted in current period: {}",
                replicas_deleted_in_current_period
            );
            let period_start = current_replicas + replicas_deleted_in_current_period;
            let period_limit = match policy.type_ {
                ScalingPolicyType::Pods => {
                    // Prevent underflow
                    if period_start >= policy.value {
                        period_start - policy.value
                    } else {
                        0
                    }
                },
                ScalingPolicyType::Percent => {
                    (period_start as f64 * (1.0 - policy.value as f64 / 100.0)).ceil() as u32
                },
            };
            result = select_fn(result, period_limit);
        }
        result
    }

    fn get_replicas_change_in_period(
        &self,
        period_seconds: u32,
        events: Option<&LinkedList<ScaleEvent>>,
    ) -> u32 {
        let period_start = Local::now() - Duration::seconds(period_seconds as i64);
        match events {
            Some(events) => events
                .iter()
                .filter(|event| event.time >= period_start.naive_utc())
                .map(|event| event.change)
                .sum(),
            None => 0,
        }
    }

    fn record_scale_event(
        &mut self,
        hpa_name: &String,
        behavior: &HorizontalPodAutoscalerBehavior,
        prev_replicas: u32,
        new_replicas: u32,
    ) {
        tracing::debug!(
            "Record scale event for {}: {} -> {}",
            hpa_name,
            prev_replicas,
            new_replicas
        );
        let now = Local::now().naive_utc();
        match new_replicas.cmp(&prev_replicas) {
            // Scaling up
            Ordering::Greater => {
                let longest_period = behavior.scale_up.longest_period();
                self.scale_up_events
                    .entry(hpa_name.to_owned())
                    .and_modify(|events| {
                        PodAutoscaler::discard_outdated_scale_events(events, longest_period)
                    })
                    .or_insert_with(LinkedList::new)
                    .push_back(ScaleEvent {
                        change: new_replicas - prev_replicas,
                        time: now,
                    });
            },
            // Scaling down
            Ordering::Less => {
                let longest_period = behavior.scale_down.longest_period();
                self.scale_down_events
                    .entry(hpa_name.to_owned())
                    .and_modify(|events| {
                        PodAutoscaler::discard_outdated_scale_events(events, longest_period)
                    })
                    .or_insert_with(LinkedList::new)
                    .push_back(ScaleEvent {
                        change: prev_replicas - new_replicas,
                        time: now,
                    });
            },
            Ordering::Equal => {},
        }
    }

    /// Remove events that are older than `now - period`.
    fn discard_outdated_scale_events(events: &mut LinkedList<ScaleEvent>, period: u32) {
        let period_start = (Local::now() - Duration::seconds(period as i64)).naive_utc();
        while events
            .front()
            .map_or(false, |event| event.time < period_start)
        {
            tracing::debug!("Discard outdated scale event: {:?}", events.front());
            events.pop_front();
        }
    }

    fn create_hpa_informer(
        tx: Sender<String>,
        resync_tx: Sender<ResyncNotification>,
    ) -> Informer<HorizontalPodAutoscaler> {
        let lw = create_lister_watcher("horizontalpodautoscalers".to_string());

        let tx_add = tx;
        let tx_update = tx_add.clone();
        let eh = EventHandler::<HorizontalPodAutoscaler> {
            add_cls: Box::new(move |new| {
                let tx_add = tx_add.clone();
                Box::pin(async move {
                    tx_add.send(new.metadata.name).await?;
                    Ok(())
                })
            }),
            update_cls: Box::new(move |(old, new)| {
                let tx_update = tx_update.clone();
                Box::pin(async move {
                    if old.spec != new.spec {
                        tx_update.send(new.metadata.name).await?;
                    }
                    Ok(())
                })
            }),
            delete_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
        };
        let rh = ResyncHandler(Box::new(move |()| {
            let resync_tx = resync_tx.clone();
            Box::pin(async move {
                resync_tx.send(ResyncNotification).await?;
                Ok(())
            })
        }));

        Informer::new(lw, eh, rh)
    }

    fn create_pod_informer() -> Informer<Pod> {
        let lw = create_lister_watcher("pods".to_string());
        let eh = EventHandler::<Pod> {
            add_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
            update_cls: Box::new(move |(_, __)| Box::pin(async move { Ok(()) })),
            delete_cls: Box::new(move |_| Box::pin(async move { Ok(()) })),
        };
        let rh = ResyncHandler(Box::new(move |()| Box::pin(async move { Ok(()) })));
        Informer::new(lw, eh, rh)
    }

    async fn handle_resync(&mut self) {
        let store = self.hpa_store.read().await;
        for hpa in store.values() {
            let hpa_name = &hpa.metadata.name;
            if !self.in_queue.contains(hpa_name) {
                self.in_queue.insert(hpa_name.to_owned());
                self.work_queue
                    .insert_at(hpa_name.to_owned(), std::time::Instant::now());
            }
        }
    }

    fn enqueue_hpa(&self, hpa_name: &String, duration: std::time::Duration) {
        if !self.in_queue.contains(hpa_name) {
            self.work_queue.insert(hpa_name.to_owned(), duration);
        }
    }
}
