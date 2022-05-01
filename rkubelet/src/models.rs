use resources::objects::KubeObject;

#[derive(Debug)]
pub enum PodUpdate {
    Add(KubeObject),
    Update(KubeObject),
    Delete(KubeObject),
}
