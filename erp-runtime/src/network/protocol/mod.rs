pub mod announce;
pub mod identify;
pub mod node_hello;
pub mod ping;
pub mod sync;

pub use announce::ANNOUNCE_PROTOCOL;
pub use identify::{ IDENTIFY_PROTOCOL, IDENTIFY_PUSH_PROTOCOL };
pub use node_hello::NODE_HELLO_PROTOCOL;
pub use ping::PING_PROTOCOL;
pub use sync::SYNC_PROTOCOL;

pub const SESSION_SETUP_PROTOCOLS: &[&str] = &[IDENTIFY_PROTOCOL, NODE_HELLO_PROTOCOL];

pub const INBOUND_SESSION_PROTOCOLS: &[&str] = &[
    IDENTIFY_PROTOCOL,
    IDENTIFY_PUSH_PROTOCOL,
    NODE_HELLO_PROTOCOL,
    PING_PROTOCOL,
    ANNOUNCE_PROTOCOL,
    SYNC_PROTOCOL,
];

pub fn advertised_protocols() -> Vec<String> {
    INBOUND_SESSION_PROTOCOLS.iter()
        .map(|protocol| (*protocol).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{ advertised_protocols, INBOUND_SESSION_PROTOCOLS, SESSION_SETUP_PROTOCOLS };

    #[test]
    fn advertised_protocols_match_inbound_protocols() {
        let unique = INBOUND_SESSION_PROTOCOLS.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), INBOUND_SESSION_PROTOCOLS.len());

        let advertised = advertised_protocols();
        assert_eq!(advertised.len(), INBOUND_SESSION_PROTOCOLS.len());
        for protocol in INBOUND_SESSION_PROTOCOLS {
            assert!(advertised.iter().any(|candidate| candidate == protocol));
        }

        for protocol in SESSION_SETUP_PROTOCOLS {
            assert!(INBOUND_SESSION_PROTOCOLS.contains(protocol));
        }
    }
}
