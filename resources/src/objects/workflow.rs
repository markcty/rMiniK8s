use std::default::Default;

use serde::{Deserialize, Serialize};

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
    start_at: String,
    /// An object containing a comma-delimited set of states.
    states: Vec<State>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStatus {
    current_state: String,
    current_result: String,
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
    Fail(Fail),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Task {
    /// Name of the Task
    name: String,
    /// Identifies the specific function to excute.
    resource: String,
    /// The next field that is run when the task state is complete.
    /// If it's None, the state will end the execution.
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Choice {
    choices: Vec<ChoiceRule>,
    default: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ChoiceRule {
    name: String,
    comparison: Comparison,
    next: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Comparison {
    StringEquals(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Fail {
    name: String,
    error: StateError,
    cause: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum StateError {
    DefaultStateError,
}
