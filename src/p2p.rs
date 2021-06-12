use async_std::{
	channel,
	channel::{Receiver, Sender},
};
use bevy::{prelude::*, tasks::IoTaskPool};
use bevy_rapier3d::prelude::*;
use bincode::deserialize;
use futures::{prelude::*, select};
use libp2p::{
	development_transport,
	gossipsub::{
		error::{PublishError, SubscriptionError},
		Gossipsub, GossipsubConfig, GossipsubEvent, Hasher, IdentTopic, MessageAuthenticity, MessageId, Topic,
	},
	identify::{Identify, IdentifyConfig, IdentifyEvent},
	identity,
	mdns::{Mdns, MdnsConfig, MdnsEvent},
	NetworkBehaviour, PeerId, Swarm,
};
use serde::{Deserialize, Serialize};

pub fn p2p(mut commands: Commands, tasks: Res<IoTaskPool>) {
	let (upload_send, upload_recv) = channel::unbounded::<P2PDto>();
	let (download_send, download_recv) = channel::unbounded::<P2PEvent>();

	commands.insert_resource(upload_send);
	commands.insert_resource(download_recv);

	tasks.spawn(p2p_task(upload_recv, download_send)).detach();
}
async fn p2p_task(mut upload_recv: Receiver<P2PDto>, download_send: Sender<P2PEvent>) {
	let local_key = identity::Keypair::generate_ed25519();
	let local_peer_id = PeerId::from(local_key.public());
	println!("Local peer id: {:?}", local_peer_id);

	let topic = IdentTopic::new("example");

	let mut swarm = {
		let transport = development_transport(local_key.clone()).await.unwrap();

		let message_authenticity = MessageAuthenticity::Signed(local_key.clone());
		let mut gossipsub = Gossipsub::new(message_authenticity, GossipsubConfig::default()).unwrap();
		gossipsub.subscribe(&topic).unwrap();

		let identify = Identify::new(IdentifyConfig::new("ipfs/1.0.0".into(), local_key.public()));

		let mdns = Mdns::new(MdnsConfig::default()).await.unwrap();

		let behaviour = MyBehaviour { gossipsub, identify, mdns };

		Swarm::new(transport, behaviour, local_peer_id)
	};
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap()).unwrap();

	loop {
		select! {
			event = swarm.select_next_some() => handle_swarm_event(event, &mut swarm, &download_send).await,
			upload = upload_recv.select_next_some() => {
				match swarm.behaviour_mut().publish(topic.clone(), bincode::serialize(&upload).unwrap()) {
					Ok(_) | Err(PublishError::InsufficientPeers) => (),
					err => panic!("{:?}", err),
				}
			},
		};
	}
}

async fn handle_swarm_event(event: Event, swarm: &mut Swarm<MyBehaviour>, download_send: &Sender<P2PEvent>) {
	match event {
		Event::MdnsEvent(event) => {
			if let MdnsEvent::Discovered(addrs) = event {
				for addr in addrs {
					println!("discovered {:?}", addr);
					swarm.dial_addr(addr.1).unwrap();
				}
			}
		},
		Event::GossipsubEvent(event) => match event {
			GossipsubEvent::Subscribed { peer_id, .. } => {
				download_send.send(P2PEvent::PlayerConnected(peer_id)).await.unwrap()
			},
			GossipsubEvent::Message { message, .. } => match deserialize(&message.data).unwrap() {
				P2PDto::PlayerUpdate(pos, vel) => {
					download_send.send(P2PEvent::PlayerUpdate(message.source.unwrap(), pos, vel)).await.unwrap()
				},
			},
			event => println!("gossipsub {:?}", event),
		},
		_ => (),
	}
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
#[behaviour(event_process = false)]
struct MyBehaviour {
	gossipsub: Gossipsub,
	identify: Identify,
	mdns: Mdns,
}
#[allow(unused)]
impl MyBehaviour {
	fn subscribe<H: Hasher>(&mut self, topic: &Topic<H>) -> Result<bool, SubscriptionError> {
		self.gossipsub.subscribe(topic)
	}

	fn publish<H: Hasher>(&mut self, topic: Topic<H>, data: impl Into<Vec<u8>>) -> Result<MessageId, PublishError> {
		self.gossipsub.publish(topic, data)
	}
}

#[derive(Debug)]
enum Event {
	GossipsubEvent(GossipsubEvent),
	IdentifyEvent(IdentifyEvent),
	MdnsEvent(MdnsEvent),
}
impl From<GossipsubEvent> for Event {
	fn from(event: GossipsubEvent) -> Self {
		Event::GossipsubEvent(event)
	}
}
impl From<IdentifyEvent> for Event {
	fn from(event: IdentifyEvent) -> Self {
		Event::IdentifyEvent(event)
	}
}
impl From<MdnsEvent> for Event {
	fn from(event: MdnsEvent) -> Self {
		Event::MdnsEvent(event)
	}
}

pub enum P2PEvent {
	PlayerConnected(PeerId),
	PlayerUpdate(PeerId, RigidBodyPosition, RigidBodyVelocity),
}

#[derive(Serialize, Deserialize)]
pub enum P2PDto {
	PlayerUpdate(RigidBodyPosition, RigidBodyVelocity),
}
