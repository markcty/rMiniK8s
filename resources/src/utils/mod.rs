use crate::objects::Labels;

pub fn selector_match(selector: &Labels, labels: &Labels) -> bool {
    for (key, value) in selector {
        if let Some(v) = labels.get(key.as_str()) {
            if value == v {
                return true;
            }
        }
    }
    false
}
