use std::{collections::HashMap, default::Default};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Metadata, Object};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Workflow {
    pub metadata: Metadata,
    pub spec: WorkflowSpec,
    pub status: Option<WorkflowStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSpec {
    /// A string that must exactly match (is case sensitive) the name of one of the state objects.
    pub start_at: String,
    /// An object containing a comma-delimited set of states.
    pub states: HashMap<String, State>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStatus {
    pub current_state: String,
    pub current_result: String,
}

impl Object for Workflow {
    fn kind(&self) -> &'static str {
        "Workflow"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum State {
    Task(Task),
    Choice(Choice),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Task {
    /// Identifies the specific function to excute.
    pub resource: String,
    /// The next field that is run when the task state is complete.
    /// If it's None, the state will end the execution.
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Choice {
    pub rules: Vec<ChoiceRule>,
    pub default: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ChoiceRule {
    #[serde(flatten)]
    pub comparison: Comparison,
    pub next: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum Comparison {
    FieldEquals { field: String, content: String },
    FieldNumEquals { field: String, content: i32 },
}

impl ChoiceRule {
    pub fn match_with(&self, text: &str) -> bool {
        let args: serde_json::Map<String, Value> = serde_json::from_str(text).unwrap();
        match self.comparison {
            Comparison::FieldEquals {
                ref field,
                ref content,
            } => {
                if let Some(Value::String(v)) = args.get(field) {
                    v == content
                } else {
                    false
                }
            },
            Comparison::FieldNumEquals {
                ref field,
                ref content,
            } => {
                if let Some(Value::Number(v)) = args.get(field) {
                    if let Some(v) = v.as_i64() {
                        v == *content as i64
                    } else {
                        false
                    }
                } else {
                    false
                }
            },
        }
    }
}
