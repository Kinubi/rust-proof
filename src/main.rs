use rust_proof::node::Node;

#[tokio::main]
async fn main() {
    println!("Starting rust-proof node...");
    let storage = Box::new(rust_proof::storage::SledStorage::new("data").unwrap());
    let (node, _sender) = Node::new(storage);
    tokio::spawn(async move {
        node.run().await;
    });

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
