use libp2p::{
  dns::{DnsConfig, TokioDnsResolver},
  gossipsub::{Gossipsub, GossipsubConfig, GossipsubEvent, MessageId},
  identity::Keypair,
  mdns::{Mdns, TokioMdns},
  noise::{Keypair as NoiseKeypair, NoiseConfig, X25519Spec},
  swarm::{Swarm, SwarmBuilder, SwarmEvent},
  tcp::TokioTcpConfig,
  Multiaddr, PeerId, Swarm, Transport,
};
use std::error::Error;
use std::time::Duration;

const BOOTSTRAP_PEER: &str = "/dns4/bootstrap.libp2p.io/tcp/443/wss/p2p-webrtc-star/p2p/16Uiu2HAm9otWzXBjk3AUfTFtS4ReqwXfGUcFHqxpnhtaTgWU8ome";
const PUBSUB_TOPIC: &str = "my-pubsub-topic";

fn build_transport(keypair: &Keypair) -> Result<impl Transport, Box<dyn Error>> {
  let noise_keys = NoiseKeypair::<X25519Spec>::new().into_authentic(keypair)?;
  let noise = NoiseConfig::xx(noise_keys).into_authenticated();
  let tcp = TokioTcpConfig::new().nodelay(true);
  let dns = DnsConfig::system(TokioDnsResolver::new().await?)?;
  let transport = dns.layer(tcp).upgrade(libp2p::upgrade::Version::V1).authenticate(noise).multiplex(libp2p::yamux::Config::default()).boxed();
  Ok(transport)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  // Generate a keypair for the local peer.
  let local_key = Keypair::generate_ed25519();
  let local_peer_id = PeerId::from(local_key.public());

  // Build the transport, swarm, and gossipsub components.
  let transport = build_transport(&local_key).await?;
  let mut swarm = {
    let mdns = TokioMdns::new(Duration::from_secs(5)).await?;
    let gossipsub_config = GossipsubConfig::default();
    let mut gossipsub = Gossipsub::new(local_peer_id.clone(), gossipsub_config);
    gossipsub.subscribe(PUBSUB_TOPIC.into());

    SwarmBuilder::new(transport, gossipsub, local_peer_id)
      .add_protocol(mdns)
      .build()
  };

  // Connect to the bootstrap peer.
  let bootstrap_addr: Multiaddr = BOOTSTRAP_PEER.parse()?;
  Swarm::dial_addr(&mut swarm, bootstrap_addr)?;

  // Begin the event loop for the node.
  loop {
    match swarm.next().await {
      // Process gossipsub events
      SwarmEvent::Behaviour(libp2p::gossipsub::GossipsubEvent::Message {
                                             message_id,
                                             propagation_source,
                                             message,
                                           }) => {
        println!(
          "Received message: '{:?}' from peer: {:?} with id: {:?}",
          String::from_utf8_lossy(&message.data),
          propagation_source,
          message_id
        );
      }

      // Process MDNS events
      SwarmEvent::Behaviour(libp2p::mdns::MdnsEvent::Discovered(list)) => {
        for (peer_id, _) in list {
          if Swarm::is_connected(&swarm, &peer_id) {
            println!("Already connected to peer: {:?}", peer_id);
          } else {
            match Swarm::dial(&mut swarm, peer_id.clone()) {
              Ok(_) => println!("Dialed peer: {:?}", peer_id),
              Err(_) => println!("Failed to dial peer: {:?}", peer_id),
            }
          }
        }
      }

      // Other events
      e => {
        println!("Unhandled event: {:?}", e);
      }
    }
  }
}

