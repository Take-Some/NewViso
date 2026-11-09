/** Shared locomotion/event integration across controller ownership and frame rates. */
import assert from "node:assert/strict";
import {readFileSync,readdirSync} from "node:fs";
import {fileURLToPath} from "node:url";
import {stripTypeScriptTypes} from "node:module";
const root=fileURLToPath(new URL("../../",import.meta.url));
const shared=root+"Shared/Content/scripts/newviso/";
const url=s=>"data:text/javascript,"+encodeURIComponent(stripTypeScriptTypes(s));
const core=url(readFileSync(shared+"character_motion.ysc","utf8"));
const {CharacterMotion}=await import(core);
const surface=url(readFileSync(shared+"compat/rsc7_surface_audio.ysc","utf8"));
const support=url(readFileSync(shared+"character_support.ysc","utf8"));
const eventSource="export const Events={events:[],emit(topic,payload){this.events.push({topic,payload});}};";
const eventUrl=url(eventSource);const {Events}=await import(eventUrl);
let bridge=readFileSync(shared+"gameplay/character_motion.ysc","utf8")
.replace('"../character_motion.ysc"',JSON.stringify(core))
.replace('"../character_support.ysc"',JSON.stringify(support))
.replace('"../compat/rsc7_surface_audio.ysc"',JSON.stringify(surface));
const api=await import(url(bridge));
let checks=0;
const check=(condition,message)=>{assert(condition,message);checks++;};
const base={actor_id:"arbitrary",scene_id:"any.skeleton",position:[0,0,0],velocity:[0,0,0],grounded:true,gait:"idle",surface:"wood",delta_seconds:1/60};
for(const hz of [30,60,144,240]){
 const tracker=new CharacterMotion(.08,2.25,.25), input={...base,delta_seconds:1/hz};
 check(tracker.update({...input,grounded:false}).events.length===0,"spawn air is silent");
 check(tracker.update(input).events.length===0,"first floor is silent");
 const launch=tracker.update({...input,grounded:false,velocity:[1,5,0]});
 check(launch.motion.jumped&&launch.events.filter(x=>x.topic==="character.jump").length===1,"one accepted lift");
 let landCount=0;
 for(let frame=0;frame<hz;frame++){
  const y=frame/hz;
  tracker.update({...input,grounded:false,position:[y,y,0],velocity:[1,5-10*y,0]});
 }
 const land=tracker.update({...input,velocity:[1,0,0],position:[1,0,0]});
 check(land.motion.landed&&land.motion.impact_speed>4,"landing preserves pre-contact impact speed");
 landCount+=land.events.filter(x=>x.topic==="character.land").length;
 landCount+=tracker.update(input).events.filter(x=>x.topic==="character.land").length;
 check(landCount===1,"landing emitted once");
 tracker.update({...input,grounded:false});
 check(!tracker.update(input).motion.landed,"substep contact flicker is silent");
 tracker.update({...input,position:[100,100,0],grounded:false,velocity:[0,5,0]});
 check(!tracker.update({...input,position:[100,0,0]}).motion.landed,"teleport resets air state");
 check(tracker.update({...input,delta_seconds:NaN}).motion===null,"invalid sample is rejected");
}
const defaults={enabled:true,default_surface:"stone",moving_speed:.12,air_confirm_seconds:.08,max_travel_per_frame:2.25,release_speed:.25,support_below_radius:.25,support_min_cosine:.55,run_threshold_ratio:.75,sprint_threshold_ratio:.83};
for(const controller of ["shared","project"]){
 const player={enabled:true,controller,physics_entity:123,scene_id:"renamed.body",body_height:2,body_radius:.3,walk_speed:1.5,animation_reference_run_speed:3,sprint_speed:4.6};
 const payload={delta_seconds:1/60,project:{runtime:{variables:{engine_character_motion:defaults,engine_player:player}}},
  runtime:{physics:{bodies:[{entity:123,position:[0,1,0],linear_velocity:[0,0,0]}],events:[{ContactPersist:{a:123,b:42,normal:[0,1,0],point:[0,0,0],surface_id:27,surface_entity:42}}]}}};
 api.startCharacterMotion(payload); Events.events.length=0;
 let c=api.frameCharacterMotion(payload);check(c[0].grounded&&c[0].surface==="snow","real support material, either controller");
 payload.runtime.physics.bodies[0].position[0]=.1;payload.runtime.physics.bodies[0].linear_velocity=[3,0,0];
 c=api.frameCharacterMotion(payload);check(c[0].gait==="run","running inferred from resolved physics");
 check(c.some(e=>e.op==="events.emit"&&e.topic==="character.locomotion.run"&&e.metadata.script_echo),"shared canonical run event broadcasts once");
 payload.runtime.physics.events.push({ContactEnd:{a:42,b:123}});
 c=api.frameCharacterMotion(payload);check(!c[0].grounded,"ended support cannot remain grounded");
 payload.runtime.physics.bodies=[];c=api.frameCharacterMotion(payload);check(c[0].op==="character.audio.reset","removed body releases tracker");
 api.stopCharacterMotion();
}
const player={enabled:true,controller:"project",physics_entity:123,scene_id:"rider",body_height:2,body_radius:.3,walk_speed:1.5,animation_reference_run_speed:3,sprint_speed:4.6};
const p={delta_seconds:1/60,project:{runtime:{variables:{engine_character_motion:defaults,engine_player:player}}},
 runtime:{physics:{bodies:[{entity:123,position:[0,1,0],linear_velocity:[2,3,0]},{entity:42,position:[0,-1,0],linear_velocity:[2,3,0]}],
 events:[{ContactPersist:{a:42,b:123,normal:[0,-1,0],point:[0,0,0]}}]}}};
api.startCharacterMotion(p);const c=api.frameCharacterMotion(p)[0];
check(c.grounded&&c.speed===0&&c.gait==="idle","moving platform does not become a jump/run");
check((await import(surface)).rageFootstepSurface(null,"snow")==="snow","unknown surface uses authored fallback");
const {characterSupport,resetCharacterSupport}=await import(support);
resetCharacterSupport();
const body={entity:700,position:[0,1,0],bounds_min:[-.3,0,-.3]};
const physics={fixed_tick:1,events:[{ContactPersist:{a:700,b:42,normal:[0,1,0],point:[0,0,0]}}]};
check(characterSupport(physics,body,.3)!==null,"physics contact established");
physics.events=[];
check(characterSupport(physics,body,.3)!==null,"render-only frame keeps support");
body.position[1]=3;body.bounds_min[1]=2;
check(characterSupport(physics,body,.3)===null,"cached floor cannot ground a jump apex");
body.position[1]=1;body.bounds_min[1]=0;physics.fixed_tick=2;physics.events=[{ContactEnd:{a:700,b:42}}];
check(characterSupport(physics,body,.3)===null,"contact end releases support on physics clock");
body.stair_support={height:0,normal:[0,1,0],entity_key:"42",lifting:true};
physics.fixed_tick=3;physics.events=[{ContactPersist:{a:700,b:42,normal:[0,1,0],point:[0,0,0],surface_entity_key:"42",surface_id:73}}];
check(characterSupport(physics,body,.3).surface_id===73,"stair transfer preserves the real tread material metadata");
const xml=readFileSync(root+"Shared/Content/config/engine/runtime.defaults.xml","utf8");
const playerXml=xml.split("<engine_player>")[1].split("</engine_player>")[0];
const scalar=name=>{const m=playerXml.match(new RegExp("<"+name+" value=\"([^\"]+)\""));assert(m,name);const x=Number(m[1]);return Number.isFinite(x)?x:m[1];};
const commonPlayer=Object.fromEntries(["physics_entity","scene_id","body_height","body_radius","walk_speed","sprint_speed","animation_reference_run_speed"].map(n=>[n,scalar(n)]));
const projects=[];
for(const entry of readdirSync(root+"Projects",{withFileTypes:true})){
 if(!entry.isDirectory()||entry.name==="legacy")continue;
 const dir=root+"Projects/"+entry.name+"/";
 let manifest;try{manifest=JSON.parse(readFileSync(dir+"project.json","utf8"));}catch{continue;}
 const runtime=JSON.parse(readFileSync(dir+manifest.files.runtime,"utf8"));
 const player={...commonPlayer,...runtime.variables?.engine_player};
 if(player.enabled===false)continue;
 const policy={...defaults,...runtime.variables?.engine_character_motion};
 const p={delta_seconds:1/60,project:{runtime:{variables:{engine_player:player,engine_character_motion:policy}}},
 runtime:{physics:{fixed_tick:1,bodies:[{entity:player.physics_entity,position:[0,player.body_height/2,0],linear_velocity:[1.5,0,0],bounds_min:[-.3,0,-.3]}],
 events:[{ContactPersist:{a:player.physics_entity,b:12,point:[0,0,0],normal:[0,1,0]}}]}}};
 api.startCharacterMotion(p);
 const cmd=api.frameCharacterMotion(p).find(c=>c.op==="character.audio.motion");
 check(cmd?.actor_id===player.scene_id&&cmd.grounded,"engine default motion subscription in "+entry.name);
 projects.push(entry.name);api.stopCharacterMotion();
}
// Check source syntax in the engine's shared graph and every active project.
for(const basePath of [shared,...readdirSync(root+"Projects",{withFileTypes:true}).filter(x=>x.isDirectory()&&x.name!=="legacy").map(x=>root+"Projects/"+x.name+"/scripts/")]){
 const walk=dir=>{let files;try{files=readdirSync(dir,{withFileTypes:true});}catch{return;}
  for(const f of files){const path=dir+f.name;if(f.isDirectory()){if(f.name!=="generated"&&f.name!=="newviso")walk(path+"/");}
   else if(f.name.endsWith(".ysc")){stripTypeScriptTypes(readFileSync(path,"utf8"),{mode:"transform"});checks++;}}};
 walk(basePath);
}
process.stdout.write(JSON.stringify({passed:true,checks,projects})+"\n");

// Stair support transfers must stay on the shared event/audio path for any controller.
for(const controller of ["shared","project"]){
 const body={entity:123,position:[0,1,0],linear_velocity:[1.2,-.08,0],bounds_min:[-.3,0,-.3],
  stair_support:{height:0,normal:[0,1,0],entity_key:"12",lifting:true}};
 const payload={delta_seconds:1/60,project:{runtime:{variables:{engine_character_motion:defaults,
  engine_player:{...commonPlayer,physics_entity:123,controller,scene_id:"stairs.actor"}}}},
  runtime:{physics:{fixed_tick:1,bodies:[body],events:[]}}};
 api.startCharacterMotion(payload);
 for(let step=0;step<8;step++){
  body.position=[step*.32,1+step*.22,0];body.bounds_min[1]=step*.22;
  body.linear_velocity[1]=step%2===0?.37:-.08;
  body.stair_support.height=step*.22;payload.runtime.physics.fixed_tick++;
  const commands=api.frameCharacterMotion(payload);
  check(commands[0]?.grounded===true,"validated stair transfer remains grounded");
  check(!commands.some(c=>c.topic==="character.jump"||c.topic==="character.fall"||c.topic==="character.land"),"stair transfer has no false air lifecycle");
 }
 api.stopCharacterMotion();
}
for(const project of ["FirstFPS","Island","ProceduralMap"]){
 const source=readFileSync(root+"Projects/"+project+"/scripts/fps/physics.ysc","utf8")
  .replace(/import \{ CFG \} from "\.\/config\.ysc";/,'const CFG={};')
  .replace('"../newviso/character_support.ysc"',JSON.stringify(support));
 const physics=await import(url(source));
 const body={position:[0,1,0],linear_velocity:[0,-2,0]};
 const intent=physics.playerMotionCommands(body,[1,2],-2);
 check(intent.length===1&&intent[0].op==="physics.character.drive","project uses the common finite drive: "+project);
 check(intent[0].jump_speed===undefined,"fall velocity is not converted into a jump: "+project);
 check(physics.playerMotionCommands(body,[1,2],5,{jump:true})[0].jump_speed===5,"intent carries an explicit lift: "+project);
 check(physics.playerMotionCommands(body,[1,2],0)[0].jump_speed===undefined,"contact recovery is not a jump: "+project);
 body.linear_velocity=[3,-2,-4];
 const water=physics.playerMotionCommands(body,[1,2],-1,{vertical:true});
 check(water.length===2&&water[0].op==="physics.body.velocity.set"&&water[0].velocity.join() === "3,-1,-4"&&water[1].op==="physics.character.drive"&&water[1].jump_speed===undefined,"water preserves native horizontal momentum without a fake jump: "+project);
}
for(const project of ["NorthYankton/scripts/player/controller.ysc","VehicleTest/scripts/main.ysc"]){
 const source=readFileSync(root+"Projects/"+project,"utf8");
 stripTypeScriptTypes(source);
 check(source.includes('op:"physics.character.drive"'),"legacy controller routes locomotion through engine: "+project);
}
console.log("STAIR_SHARED_ADAPTER_CHECKS_PASS",checks);
