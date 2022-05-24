use resources::objects::pod::Pod;

#[derive(Debug)]
pub enum PodUpdate {
    Add(Pod),
    Update(Pod, Pod),
    Delete(Pod),
}
