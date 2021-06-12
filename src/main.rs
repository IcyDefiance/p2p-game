mod p2p;

use crate::p2p::{p2p, P2PDto, P2PEvent};
use async_std::channel::{Receiver, Sender};
use bevy::{
	core::FixedTimestep,
	pbr::LightBundle,
	prelude::{shape, *},
	utils::HashMap,
};
use bevy_rapier3d::prelude::*;
use libp2p::PeerId;

fn main() {
	let peers = HashMap::<PeerId, Entity>::default();

	App::build()
		.add_plugins(DefaultPlugins)
		.add_plugin(RapierPhysicsPlugin::<NoUserData>::default())
		.insert_resource(peers)
		.add_startup_system(p2p.system())
		.add_startup_system(world.system())
		.add_system(network_events.system())
		.add_system(input.system())
		.add_system_set(
			SystemSet::new()
				.with_run_criteria(FixedTimestep::steps_per_second(20.0))
				.with_system(publish_state.system()),
		)
		.run();
}

fn network_events(
	mut commands: Commands,
	recv: Res<Receiver<P2PEvent>>,
	player_spawner: Res<PlayerSpawner>,
	mut peers: ResMut<HashMap<PeerId, Entity>>,
) {
	while let Ok(msg) = recv.try_recv() {
		match msg {
			P2PEvent::PlayerConnected(peer_id) => {
				let entity = player_spawner.spawn(&mut commands, Some(peer_id));
				peers.insert(peer_id, entity);
			},
			P2PEvent::PlayerUpdate(peer_id, pos, vel) => {
				commands.entity(peers[&peer_id]).insert_bundle((pos, vel));
			},
		}
	}
}

fn input(keys: Res<Input<KeyCode>>, mut player: Query<&mut RigidBodyVelocity, With<PlayerController>>) {
	let mut input = Vec3::ZERO;

	if keys.pressed(KeyCode::W) {
		input -= Vec3::Z;
	}
	if keys.pressed(KeyCode::A) {
		input -= Vec3::X;
	}
	if keys.pressed(KeyCode::S) {
		input += Vec3::Z;
	}
	if keys.pressed(KeyCode::D) {
		input += Vec3::X;
	}

	input *= 0.1;
	for mut vel in player.iter_mut() {
		vel.linvel += &input.into();
		vel.linvel = vel.linvel.cap_magnitude(5.0);
	}
}

fn publish_state(
	send: Res<Sender<P2PDto>>,
	players: Query<(&RigidBodyPosition, &RigidBodyVelocity), With<PlayerController>>,
) {
	for (&pos, &vel) in players.iter() {
		send.try_send(P2PDto::PlayerUpdate(pos, vel)).unwrap();
	}
}

fn world(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
	let collider = ColliderBundle { shape: ColliderShape::cuboid(5.0, 0.01, 5.0), ..Default::default() };
	commands.spawn_bundle(collider).insert_bundle(PbrBundle {
		mesh: meshes.add(Mesh::from(shape::Plane { size: 10.0 })),
		material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
		..Default::default()
	});

	let player_spawner = PlayerSpawner {
		mesh: meshes.add(Mesh::from(shape::Capsule::default())),
		material: materials.add(Color::rgb(0.8, 0.7, 0.6).into()),
	};

	player_spawner.spawn(&mut commands, None);

	commands.insert_resource(player_spawner);

	commands.spawn_bundle(LightBundle { transform: Transform::from_xyz(4.0, 8.0, 4.0), ..Default::default() });

	commands.spawn_bundle(PerspectiveCameraBundle {
		transform: Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
		..Default::default()
	});
}

struct PlayerController;

struct PlayerSpawner {
	mesh: Handle<Mesh>,
	material: Handle<StandardMaterial>,
}
impl PlayerSpawner {
	fn spawn(&self, commands: &mut Commands, peer_id: Option<PeerId>) -> Entity {
		let mut commands = commands.spawn_bundle(RigidBodyBundle {
			position: Vec3::new(0.0, 2.0, 0.0).into(),
			mass_properties: RigidBodyMassProps {
				flags: RigidBodyMassPropsFlags::ROTATION_LOCKED,
				..Default::default()
			},
			..Default::default()
		});
		commands
			.insert_bundle(ColliderBundle {
				shape: ColliderShape::capsule(Point::new(0.0, -0.5, 0.0), Point::new(0.0, 1.5, 0.0), 0.5),
				..Default::default()
			})
			.insert_bundle(PbrBundle { mesh: self.mesh.clone(), material: self.material.clone(), ..Default::default() })
			.insert(ColliderPositionSync::Discrete);
		if let Some(peer_id) = peer_id {
			commands.insert(peer_id);
		} else {
			commands.insert(PlayerController);
		}
		commands.id()
	}
}
