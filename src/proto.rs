use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkType {
    Send,
    Recv,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PerfRequest {
    pub work_type: WorkType,
}
