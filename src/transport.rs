
use serde::{Serialize, Deserialize};

use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct MakeNode {
    #[serde(rename = "node")]
    pub node_id: u64,
    #[serde(rename = "type")]
    pub node_type: String,
}

#[derive(Debug, Deserialize)]
pub struct Destroy {
    #[serde(rename = "node")]
    pub node_id: u64,
}

#[derive(Debug, Deserialize)]
pub struct Add {
    #[serde(rename = "node")]
    pub node_id: u64,
    #[serde(rename = "attr")]
    pub attribute: String,
    pub index: usize,
    pub value: Value
}

#[derive(Debug, Deserialize)]
pub struct Remove {
    #[serde(rename = "node")]
    pub node_id: u64,
    #[serde(rename = "attr")]
    pub attribute: String,
    pub value: Value
}

#[derive(Debug, Deserialize)]
pub struct SetAttr {
    #[serde(rename = "node")]
    pub node_id: u64,
    #[serde(rename = "attr")]
    pub attribute: String,
    pub value: Value
}

#[derive(Debug, Deserialize)]
#[serde(tag = "update-type")]
pub enum Update {
    #[serde(rename = "make-node")]
    MakeNode(MakeNode),
    #[serde(rename = "destroy")]
    Destroy(Destroy),
    #[serde(rename = "add")]
    Add(Add),
    #[serde(rename = "remove")]
    Remove(Remove),
    #[serde(rename = "set-attr")]
    SetAttr(SetAttr)

}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum UpdateOrLogId {
    Update(Update),
    LogIds(Vec<u64>),
}

pub type NoriaUpdates = Vec<UpdateOrLogId>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let json = r#"
        [42,
         {
           "update-type": "make-node",
           "type": "foo",
           "node": 42
         },
         {
           "update-type": "make-node",
           "type": "bar",
           "node": 43
         }]
        "#;
        let updates = serde_json::from_str::<Update>(json).unwrap();
    }

}