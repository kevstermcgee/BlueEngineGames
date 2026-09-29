//! Wrecking Orbit rules: drag two spring-tethered wrecking balls through crystal targets.
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller, ControllerState, Movement};
use vesper3d::viewer::devkit::{Rng, Simulation, Snapshot, StateHasher, TICK};

pub const PLATFORM_HALF:f32=8.; pub const VOID_Y:f32=-8.; pub const ORBS:usize=6;
pub const BUMPER_SPEED:f32=12.; pub const KNOCKBACK:f32=3.;

#[derive(Clone,Copy,Debug,Default,PartialEq,Serialize,Deserialize)]
#[serde(default,deny_unknown_fields)]
pub struct Input{pub forward:f32,pub right:f32,pub look:[f32;2],pub jump:bool}

#[derive(Clone,Debug,PartialEq)]
pub enum Event{Collected{at:V,score:u32},Bumped{at:V},Jumped,Landed,Fell,Won}

#[derive(Clone,Copy,Debug,PartialEq,Serialize,Deserialize)]
pub struct Bumper{pub pos:V,pub vel:V}

pub struct Sim{pub tick:u64,pub player:Controller,pub orbs:Vec<V>,pub bumpers:Vec<Bumper>,pub score:u32,pub over:bool,
    ground:Collider,rng:Rng,events:Vec<Event>,was_grounded:bool,bump_cooldown:u32,jump_was_down:bool}

impl Sim{
 pub fn new(seed:u64)->Self{
  let player=Controller::for_profile(Default::default(),V(0.,0.,0.),0.).expect("valid profile");
  Self{tick:0,player,orbs:vec![V(-5.,0.7,-4.),V(0.,0.7,-5.),V(5.,0.7,-4.),V(-5.,0.7,4.),V(0.,0.7,5.),V(5.,0.7,4.)],
   bumpers:vec![Bumper{pos:V(2.8,0.65,0.),vel:V(0.,0.,5.)},Bumper{pos:V(-2.8,0.65,0.),vel:V(0.,0.,-5.)}],score:0,over:false,
   ground:Collider{min:V(-PLATFORM_HALF,-1.,-PLATFORM_HALF),max:V(PLATFORM_HALF,0.,PLATFORM_HALF)},rng:Rng::new(seed),
   events:Vec::new(),was_grounded:true,bump_cooldown:0,jump_was_down:false}
 }
 pub fn step(&mut self,input:&Input){
  if self.over{return} self.tick+=1; self.player.look(input.look[0],input.look[1],1.,false);
  self.player.update(Movement{forward:input.forward,right:input.right,jump:false,..Default::default()},TICK,std::slice::from_ref(&self.ground));
  let anchor=V(self.player.position.0,0.65,self.player.position.2);
  let kick=input.jump&&!self.jump_was_down; self.jump_was_down=input.jump;
  if kick{self.events.push(Event::Jumped)}
  for (i,ball) in self.bumpers.iter_mut().enumerate(){
   let delta=anchor-ball.pos; let dist=delta.length().max(0.01); let stretch=dist-3.;
   ball.vel=ball.vel+delta.norm()*(stretch*18.)*TICK;
   if kick{let tangent=V(-delta.2,0.,delta.0).norm()*(if i==0{7.}else{-7.});ball.vel=ball.vel+tangent;}
   ball.vel=ball.vel*0.996;
   if ball.vel.length()>BUMPER_SPEED{ball.vel=ball.vel.norm()*BUMPER_SPEED}
   ball.pos=ball.pos+ball.vel*TICK;
   if ball.pos.0.abs()>PLATFORM_HALF-0.5{ball.pos.0=ball.pos.0.clamp(-7.5,7.5);ball.vel.0*=-0.8}
   if ball.pos.2.abs()>PLATFORM_HALF-0.5{ball.pos.2=ball.pos.2.clamp(-7.5,7.5);ball.vel.2*=-0.8}
  }
  let mut hit=None;
  'hits:for (ti,target) in self.orbs.iter().enumerate(){for ball in &self.bumpers{if (ball.pos-*target).length()<0.95&&ball.vel.length()>2.{hit=Some((ti,*target));break 'hits}}}
  if let Some((i,at))=hit{self.orbs.remove(i);self.score+=1;self.events.push(Event::Collected{at,score:self.score});
   if self.orbs.is_empty(){self.over=true;self.events.push(Event::Won)}}
  self.bump_cooldown=self.bump_cooldown.saturating_sub(1);
  if self.bump_cooldown==0{if let Some(ball)=self.bumpers.iter().find(|b|(b.pos-anchor).length()<0.8){
   let away=(anchor-ball.pos).norm();self.player.apply_impulse(away*KNOCKBACK+V(0.,1.5,0.));self.bump_cooldown=24;self.events.push(Event::Bumped{at:ball.pos})}}
 }
 pub fn drain_events(&mut self)->Vec<Event>{std::mem::take(&mut self.events)}
}

impl Simulation for Sim{type Input=Input;fn step(&mut self,i:&Input){Sim::step(self,i)}fn state_hash(&self)->u64{
 let mut h=StateHasher::new();h.u64(self.tick).u32(self.score).bool(self.over);let p=self.player.position;h.f32(p.0).f32(p.2);
 for b in &self.bumpers{h.f32(b.pos.0).f32(b.pos.2).f32(b.vel.0).f32(b.vel.2);}for o in &self.orbs{h.f32(o.0).f32(o.2);}h.finish()}}

#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
pub struct SimState{pub tick:u64,pub player:ControllerState,pub orbs:Vec<V>,pub bumpers:Vec<Bumper>,pub score:u32,pub over:bool,
 pub rng:Rng,pub was_grounded:bool,pub bump_cooldown:u32,pub jump_was_down:bool}
impl Snapshot for Sim{const KIND:&'static str="wrecking-orbit";type State=SimState;
 fn capture(&self)->SimState{SimState{tick:self.tick,player:self.player.network_state(),orbs:self.orbs.clone(),bumpers:self.bumpers.clone(),score:self.score,
  over:self.over,rng:self.rng.clone(),was_grounded:self.was_grounded,bump_cooldown:self.bump_cooldown,jump_was_down:self.jump_was_down}}
 fn restore(&mut self,s:SimState)->Result<(),String>{if s.orbs.len()>ORBS||s.bumpers.len()!=2{return Err("invalid wrecking rig".into())}
  self.tick=s.tick;self.player.restore_network_state(&s.player);self.orbs=s.orbs;self.bumpers=s.bumpers;self.score=s.score;self.over=s.over;
  self.rng=s.rng;self.was_grounded=s.was_grounded;self.bump_cooldown=s.bump_cooldown;self.jump_was_down=s.jump_was_down;self.events.clear();Ok(())}
 fn save_tick(&self)->u64{self.tick}}
