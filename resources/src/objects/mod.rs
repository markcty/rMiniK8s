use std::{collections::HashMap, fmt::Debug};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::Display;
use uuid::Uuid;
pub mod binding;
pub mod node;
pub mod object_reference;
pub mod pod;
pub mod service;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct KubeObject {
    pub metadata: Metadata,
    #[serde(flatten)]
    pub resource: KubeResource,
}

#[derive(Debug, Serialize, Deserialize, Display, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum KubeResource {
    Pod(pod::Pod),
    Binding(binding::Binding),
    Node(node::Node),
    Service(service::Service),
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
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
    pub labels: Labels,
}

pub type Labels = HashMap<String, String>;

impl KubeObject {
    pub fn kind(&self) -> String {
        self.resource.to_string().to_lowercase()
    }
    pub fn name(&self) -> String {
        self.metadata.name.to_owned()
    }
}

pub trait Object:
    Clone + Serialize + DeserializeOwned + Send + Sync + Debug + PartialEq + 'static
{
    fn uri(&self) -> String;
}

impl Object for KubeObject {
    fn uri(&self) -> String {
        format!(
            "/api/v1/{}s/{}",
            self.kind().to_lowercase(),
            self.name().to_lowercase()
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Labels(pub HashMap<String, String>);

impl ToString for Labels {
    /// Returns a string representation of the labels.
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

impl TryFrom<String> for Labels {
    type Error = anyhow::Error;

    /// Parse a string of the form "key1=value1,key2=value2" into a Labels.
    ///
    /// # Examples
    /// ```rust
    /// use resources::objects::Labels;
    ///
    /// let labels = Labels::try_from("key1=value1,key2=value2".to_string()).unwrap();
    /// assert_eq!(labels.0.get(&"key1".to_string()), Some(&"value1".to_string()));
    /// assert_eq!(labels.0.get(&"key2".to_string()), Some(&"value2".to_string()));
    /// ```
    ///
    /// # Errors
    /// ```rust
    /// use resources::objects::Labels;
    ///
    /// let labels = Labels::try_from("key1=value1,key2".to_string());
    /// assert!(labels.is_err());
    /// ```
    fn try_from(s: String) -> Result<Labels> {
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
    /// let labels1 = Labels::try_from("app=frontend,env=prod".to_string()).unwrap();
    /// let labels2 = Labels::try_from("app=frontend".to_string()).unwrap();
    /// assert!(labels1.matches(&labels2));
    ///
    /// let labels2 = Labels::new();
    /// assert!(labels1.matches(&labels2));
    ///
    /// let labels1 = Labels::try_from("app=frontend,env=prod".to_string()).unwrap();
    /// let labels2 = Labels::try_from("app=frontend,env=dev".to_string()).unwrap();
    /// assert!(!labels1.matches(&labels2));
    ///
    /// let labels1 = Labels::try_from("app=frontend".to_string()).unwrap();
    /// let labels2 = Labels::try_from("app=frontend,env=prod".to_string()).unwrap();
    /// assert!(!labels1.matches(&labels2));
    /// ```
    pub fn matches(&self, labels: &Labels) -> bool {
        labels
            .0
            .iter()
            .all(|(k, v)| self.0.get(k).map(|v2| v == v2).unwrap_or(false))
    }
}
