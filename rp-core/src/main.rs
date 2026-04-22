use rp_core::node::Node;

#[tokio::main]
async fn main() {
    println!("Starting rust-proof node...");
    let storage = Box::new(rp_core::storage::SledStorage::new("data").unwrap());
    let (node, sender) = Node::new(storage);
    tokio::spawn(async move {
        node.run().await;
    });

    let responder = tokio::sync::oneshot::channel();
    sender
        .send(rp_core::node::NodeCommand::GetLatestBlock { responder: responder.0 }).await
        .unwrap();

    println!("Latest block: {:?}", responder.1.await.unwrap());

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
