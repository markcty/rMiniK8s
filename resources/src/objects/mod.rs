use std::{collections::HashMap, fmt::Debug};

use anyhow::{Context, Result};
use enum_dispatch::enum_dispatch;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use self::object_reference::ObjectReference;

pub mod binding;
pub mod function;
pub mod gpu_job;
pub mod hpa;
pub mod ingress;
pub mod metrics;
pub mod node;
pub mod object_reference;
pub mod pod;
pub mod replica_set;
pub mod service;
pub mod workflow;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[enum_dispatch(Object)]
#[serde(tag = "kind")]
pub enum KubeObject {
    Pod(pod::Pod),
    Binding(binding::Binding),
    Node(node::Node),
    Service(service::Service),
    ReplicaSet(replica_set::ReplicaSet),
    Ingress(ingress::Ingress),
    HorizontalPodAutoscaler(hpa::HorizontalPodAutoscaler),
    GpuJob(gpu_job::GpuJob),
    Function(function::Function),
    Workflow(workflow::Workflow),
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// Name must be unique within a namespace.
    /// Is required when creating resources,
    /// although some resources may allow a client
    /// to request the generation of an appropriate name automatically.
    /// Name is primarily intended for creation idempotence
    /// and configuration definition. Cannot be updated.
    pub name: String,
    /// UID is the unique in time and space value for this object.
    /// It is typically generated by the server
    /// on successful creation of a resource
    /// and is not allowed to change on PUT operations.
    /// Populated by the system. Read-only.
    pub uid: Option<Uuid>,
    /// Map of string keys and values
    /// that can be used to organize and categorize (scope and select) objects.
    /// May match selectors of replication controllers and services.
    #[serde(default)]
    pub labels: Labels,
    /// List of objects depended by this object.
    /// If ALL objects in the list have been deleted,
    /// this object will be garbage collected.
    #[serde(default)]
    pub owner_references: Vec<ObjectReference>,
}

#[enum_dispatch]
pub trait Object:
    Clone + Serialize + DeserializeOwned + Send + Sync + Debug + PartialEq + 'static
{
    /// Return the kind of the object
    /// e.g. "Pod"
    fn kind(&self) -> &'static str;

    /// Return the kind of the object in plural
    /// e.g. "Pods"
    fn kind_plural(&self) -> String {
        if self.kind() == "ingress" {
            self.kind().to_string() + "es"
        } else {
            self.kind().to_string() + "s"
        }
    }

    /// Return the name of the object
    fn name(&self) -> &String;

    /// Return the prefix of the object in Etcd,
    /// e.g. Pod -> "/api/v1/pods"
    fn prefix(&self) -> String {
        format!("/api/v1/{}", self.kind_plural().to_lowercase())
    }

    /// Return the URI of the project in Etcd
    /// e.g. Pod "nginx" -> "/api/v1/pods/nginx"
    fn uri(&self) -> String {
        format!("{}/{}", self.prefix(), self.name())
    }

    /// Return an object reference to this object
    fn object_reference(&self) -> ObjectReference {
        ObjectReference {
            name: self.name().to_owned(),
            kind: self.kind().to_owned(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct Labels(pub HashMap<String, String>);

impl ToString for Labels {
    /// Returns a string representation of the labels.
    ///
    /// The string is formatted as a comma-separated list of key=value pairs.
    /// If the map is empty, returns a empty string.
    ///
    /// # Examples
    /// ```
    /// use resources::objects::Labels;
    /// let labels = Labels::new();
    /// assert_eq!(labels.to_string(), "");
    /// let mut labels = Labels::new();
    /// labels.insert("app", "frontend");
    /// assert_eq!(labels.to_string(), "app=frontend");
    /// let mut labels = Labels::new();
    /// labels.insert("app", "frontend").insert("env", "prod");
    /// assert!(
    ///     labels.to_string() == "app=frontend,env=prod" ||
    ///     labels.to_string() == "env=prod,app=frontend"
    /// );
    /// ```
    fn to_string(&self) -> String {
        // Join key and value with '=', join entries with ','
        self.0
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl TryFrom<&String> for Labels {
    type Error = anyhow::Error;

    /// Parse a string of the form "key1=value1,key2=value2" into a Labels.
    ///
    /// # Examples
    /// ```rust
    /// use resources::objects::Labels;
    ///
    /// let labels = Labels::try_from(&"key1=value1,key2=value2".to_string()).unwrap();
    /// assert_eq!(labels.0.get(&"key1".to_string()), Some(&"value1".to_string()));
    /// assert_eq!(labels.0.get(&"key2".to_string()), Some(&"value2".to_string()));
    /// ```
    ///
    /// # Errors
    /// ```rust
    /// use resources::objects::Labels;
    ///
    /// let labels = Labels::try_from(&"key1=value1,key2".to_string());
    /// assert!(labels.is_err());
    /// ```
    fn try_from(s: &String) -> Result<Labels> {
        let mut labels = HashMap::new();
        for label in s.split(',') {
            let mut parts = label.split('=');
            let key = parts
                .next()
                .with_context(|| "Missing label name".to_string())?;
            let value = parts
                .next()
                .with_context(|| "Missing label value".to_string())?;
            labels.insert(key.to_owned(), value.to_owned());
        }
        Ok(Labels(labels))
    }
}

impl Labels {
    pub fn new() -> Labels {
        Labels(HashMap::new())
    }

    pub fn insert(&mut self, key: &str, value: &str) -> &mut Self {
        self.0.insert(key.to_owned(), value.to_owned());
        self
    }

    /// If entries of `labels` is a subset of this
    ///
    /// # Examples
    /// ```rust
    /// use resources::objects::Labels;
    ///
    /// let labels1 = Labels::try_from(&"app=frontend,env=prod".to_string()).unwrap();
    /// let labels2 = Labels::try_from(&"app=frontend".to_string()).unwrap();
    /// assert!(labels1.matches(&labels2));
    ///
    /// let labels2 = Labels::new();
    /// assert!(labels1.matches(&labels2));
    ///
    /// let labels1 = Labels::try_from(&"app=frontend,env=prod".to_string()).unwrap();
    /// let labels2 = Labels::try_from(&"app=frontend,env=dev".to_string()).unwrap();
    /// assert!(!labels1.matches(&labels2));
    ///
    /// let labels1 = Labels::try_from(&"app=frontend".to_string()).unwrap();
    /// let labels2 = Labels::try_from(&"app=frontend,env=prod".to_string()).unwrap();
    /// assert!(!labels1.matches(&labels2));
    /// ```
    pub fn matches(&self, labels: &Labels) -> bool {
        labels
            .0
            .iter()
            .all(|(k, v)| self.0.get(k).map(|v2| v == v2).unwrap_or(false))
    }
}
