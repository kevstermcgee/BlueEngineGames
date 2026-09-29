use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic,snapshot};
use wrecking_orbit::{Event,Input,Sim};

fn script()->Vec<Input>{(0..360).map(|t|Input{forward:if t<180{1.}else{-1.},right:0.25,jump:t==30||t==210,..Default::default()}).collect()}

#[test]fn replay_is_deterministic(){assert_deterministic(||Sim::new(5),&script())}

#[test]fn tether_pulls_a_displaced_ball_back(){let mut s=Sim::new(1);s.bumpers[0].pos=V(7.,0.65,0.);let before=s.bumpers[0].pos.0;
 for _ in 0..60{s.step(&Input::default())}assert!(s.bumpers[0].pos.0<before)}

#[test]fn whip_changes_tangential_velocity(){let mut s=Sim::new(2);let before=s.bumpers[0].vel;s.step(&Input{jump:true,..Default::default()});
 assert_ne!(s.bumpers[0].vel,before);assert!(s.drain_events().contains(&Event::Jumped))}

#[test]fn a_fast_ball_shatters_a_crystal(){let mut s=Sim::new(3);s.bumpers[0].pos=s.orbs[0];s.bumpers[0].vel=V(4.,0.,0.);s.step(&Input::default());
 assert_eq!(s.score,1);assert!(s.drain_events().iter().any(|e|matches!(e,Event::Collected{..})))}

#[test]fn save_resume_is_exact(){snapshot::assert_resumes_exactly(||Sim::new(7),&script(),120)}
