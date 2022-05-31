use anyhow::{anyhow, Result};
use futures::future::join_all;
use hyper::Body;
use ordinal::Ordinal;
use reqwest::Client;
use resources::{
    models::Response,
    objects::{
        function::Function,
        workflow::{State, Workflow},
        KubeObject,
    },
};

use crate::{route::activate_rs, CONFIG};

pub async fn handle_workflow(name: &str, init_args: String) -> Result<hyper::Response<Body>> {
    let workflow = get_workflow(name).await?;

    optimize_workflow(&workflow).await?;

    // walk the flow
    let mut next = workflow
        .spec
        .states
        .get(&workflow.spec.start_at)
        .map(|state| (&workflow.spec.start_at, state));
    let mut body = init_args;
    let mut final_res = format!("Workflow \"{}\" begins!\n\n", name);
    let mut i = 1;
    let mut succeed = true;
    while let Some((state_name, state)) = next {
        if let State::Task(task) = state {
            // get the function
            let func = match get_func(task.resource.as_str()).await {
                Ok(func) => func,
                Err(e) => {
                    final_res.push_str(format!(" Failed, caused by: {}", e).as_str());
                    break;
                },
            };
            let host = func.status.unwrap().host;

            final_res.push_str(
                format!(
                    "{} State \"{}\": Task, function: {}, host: {}\n",
                    Ordinal(i),
                    state_name,
                    func.metadata.name,
                    host
                )
                .as_str(),
            );
            final_res.push_str(format!("  args: {}\n", body).as_str());

            // call the function
            body = match call_func(host.as_str(), body.to_owned()).await {
                Ok(res) => {
                    final_res.push_str("  Succeeded!\n\n\n");
                    res
                },
                Err(e) => {
                    final_res.push_str(format!("  Failed, caused by: {}\n", e).as_str());
                    succeed = false;
                    break;
                },
            };

            // get next state
            if let Some(ref state_name) = task.next {
                if let Some(state) = workflow.spec.states.get(state_name) {
                    next = Some((state_name, state));
                } else {
                    next = None;
                }
            } else {
                next = None;
            }
        } else if let State::Choice(choice) = state {
            final_res
                .push_str(format!("{} State \"{}\": Choice\n", Ordinal(i), state_name).as_str());
            final_res.push_str(format!("  Last response: {}\n", body).as_str());

            if !choice.rules.iter().any(|rule| {
                if rule.match_with(body.as_str()) {
                    final_res.push_str("  Rule matched!");
                    next = workflow
                        .spec
                        .states
                        .get(&rule.next)
                        .map(|state| (&workflow.spec.start_at, state));
                    true
                } else {
                    false
                }
            }) {
                final_res.push_str("  No Rule matched! Fallback to default.\n\n\n");
                next = workflow
                    .spec
                    .states
                    .get(&choice.default)
                    .map(|state| (&workflow.spec.start_at, state));
            }
        }
        i += 1;
    }
    if succeed {
        final_res.push_str("Workflow succesfully finished! The final response is:\n  ");
        final_res.push_str(body.as_str());
    } else {
        final_res.push_str("Workflow failed!\n");
    }

    Ok(hyper::Response::new(Body::from(final_res)))
}

async fn optimize_workflow(workflow: &Workflow) -> Result<()> {
    let mut activations = vec![];
    for (_, state) in workflow.spec.states.iter() {
        if let State::Task(task) = state {
            activations.push(activate_rs(task.resource.as_str()));
        }
    }
    join_all(activations).await;
    Ok(())
}

async fn get_workflow(name: &str) -> Result<Workflow> {
    let client = Client::new();
    let url = CONFIG
        .api_server_endpoint
        .join(format!("api/v1/workflows/{}", name).as_str())?;
    let res = client
        .get(url)
        .send()
        .await?
        .json::<Response<KubeObject>>()
        .await?;

    if let KubeObject::Workflow(workflow) = res
        .data
        .ok_or_else(|| anyhow!("Failed to get workflow object"))?
    {
        Ok(workflow)
    } else {
        Err(anyhow!("Failed to get workflow object"))
    }
}

async fn get_func(name: &str) -> Result<Function> {
    let client = Client::new();
    let url = CONFIG
        .api_server_endpoint
        .join(format!("api/v1/functions/{}", name).as_str())?;
    let res = client
        .get(url)
        .send()
        .await?
        .json::<Response<KubeObject>>()
        .await?;
    if let KubeObject::Function(function) = res
        .data
        .ok_or_else(|| anyhow!("Failed to get workflow object"))?
    {
        Ok(function)
    } else {
        Err(anyhow!("Failed to get workflow object"))
    }
}

async fn call_func(host: &str, body: String) -> Result<String> {
    let client = Client::new();
    let url = format!("http://{}", host);
    let res = client
        .get(url)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await?
        .text()
        .await?;
    Ok(res)
}
