/** Shared physical door geometry and protective input ownership. */
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import {fileURLToPath} from "node:url";
import {stripTypeScriptTypes} from "node:module";
const root=fileURLToPath(new URL("../../",import.meta.url));
const importYsc=async path=>import("data:text/javascript,"+encodeURIComponent(stripTypeScriptTypes(readFileSync(root+path,"utf8"))));
const {doorCommands,sceneDoorCommands,startDoors,stopDoors}=await importYsc("Shared/Content/scripts/newviso/world/doors.ysc");
const {protectiveHandsReserved}=await importYsc("Shared/Content/scripts/newviso/gameplay/protection.ysc");
const xml=readFileSync(root+"Shared/Content/config/engine/runtime.defaults.xml","utf8").split("<engine_doors>")[1].split("</engine_doors>")[0];
const profile={};
for(const m of xml.matchAll(/<([a-z_]+) value="([^"]+)"\s*\/>/g))profile[m[1]]=Number.isFinite(Number(m[2]))?Number(m[2]):m[2];
for(const m of xml.matchAll(/<([a-z_]+) type="array">([\s\S]*?)<\/\1>/g))
 profile[m[1]]=[...m[2].matchAll(/value="([^"]+)"/g)].map(x=>Number(x[1]));
const spec={id:"sample",profile:"swing",hinge_position:[3,2,-7],size:[1,2,.06],rotation_degrees:[0,0,0],hinge_side:-1,joint_ids:[701,702],frame:true};
let checks=0;const check=(ok,message)=>{assert(ok,message);checks++;};
function rotate([x,y,z,w],v){
 const t=[2*(y*v[2]-z*v[1]),2*(z*v[0]-x*v[2]),2*(x*v[1]-y*v[0])];
 return [v[0]+w*t[0]+y*t[2]-z*t[1],v[1]+w*t[1]+z*t[0]-x*t[2],v[2]+w*t[2]+x*t[1]-y*t[0]];
}
for(const side of [-1,1])for(const yaw of [0,35,90,180,-135]){
 const data={...spec,id:"sample."+side+"."+yaw,hinge_side:side,rotation_degrees:[0,yaw,0]};
 const commands=doorCommands(data,{swing:profile});
 const leaf=commands.find(x=>x.op==="physics.body.upsert"&&x.id===data.id);
 const hinge=commands.find(x=>x.op==="physics.hinge.upsert");
 const offset=rotate(leaf.rotation,hinge.child_pivot);
 check(leaf.position.every((v,i)=>Math.abs(v+offset[i]-data.hinge_position[i])<1e-8),"rotated hinge stays on its authored edge");
 check(leaf.body_kind==="dynamic"&&leaf.mass_properties.mass===profile.mass_kg,"leaf has real inertia");
 check(!commands.some(x=>x.op==="physics.body.pose.set"),"opening is never scripted");
 check(commands.filter(x=>x.op==="scene.dynamic_entity.upsert").every(x=>x.solid===false),"no closed-pose static collider");
}
stopDoors();
check(startDoors({project:{runtime:{variables:{engine_doors:{enabled:false,profiles:{swing:profile},doors:[spec]}}}}}).length===0,"disabled layer stays empty");
const settings={project:{runtime:{variables:{engine_doors:{enabled:true,profiles:{swing:profile},doors:[spec]}}}}};
startDoors(settings);
const cleanup=stopDoors();
check(cleanup.some(x=>x.op==="physics.hinge.remove")&&cleanup.filter(x=>x.op==="physics.body.destroy").length===5,"joint and bodies retire together");
check(stopDoors().length===0,"teardown is idempotent");
assert.throws(()=>doorCommands({...spec,joint_ids:[701,701]},{swing:profile}));checks++;
assert.throws(()=>doorCommands({...spec,size:[1,0,.06]},{swing:profile}));checks++;
const p={project:{runtime:{variables:{engine_player:{scene_id:"player"}}}},runtime:{endorphin:{protection:{actors:[{actor_id:"player",combat_reserved:true},{actor_id:"npc",combat_reserved:false}]}}}};
check(protectiveHandsReserved(p)&&!protectiveHandsReserved(p,"npc"),"reservation follows the actual actor");
p.runtime.endorphin.protection.actors[0].combat_reserved=false;
check(!protectiveHandsReserved(p),"released hands restore input");


const sceneSpec={id:"existing.mesh",profile:"swing",anchor_entity:801,joint_ids:[802,803],local_pivot:[0,0,0],frame_rotation:[0,0,0,1],collision_inset:[0,0,0]};
const sceneCommands=sceneDoorCommands(sceneSpec,{swing:profile});
check(sceneCommands.length===1&&sceneCommands[0].op==="physics.hinge.bind_scene","existing meshes bind through the engine");
check(!sceneCommands.some(x=>x.op.startsWith("scene.")),"binding does not replace the authored visual");
const sceneCleanup=stopDoors();
check(sceneCleanup.length===2&&sceneCleanup[0].op==="door.remove"&&sceneCleanup[1].op==="physics.hinge.unbind_scene","unbind preserves the existing scene object");
assert.throws(()=>sceneDoorCommands({...sceneSpec,collision_bounds:[[0,0,0],[1,0,1]]},{swing:profile}));checks++;
assert.throws(()=>sceneDoorCommands({...sceneSpec,collision_bounds:[[0,0,0],[1,2,Infinity]]},{swing:profile}));checks++;
const fitted=sceneDoorCommands({...sceneSpec,collision_bounds:[[0,-1,-.03],[1,1,.03]]},{swing:profile})[0];
check(fitted.collision_bounds[0][1]===-1,"authored panel bounds cross the neutral engine boundary");
stopDoors();
const firstRuntime=JSON.parse(readFileSync(root+"Projects/FirstFPS/config/runtime.json","utf8"));
const firstScene=JSON.parse(readFileSync(root+"Projects/FirstFPS/scenes/lifeinvader_rsc7.scene.json","utf8"));
const sceneNames=new Map(firstScene.entities.map(x=>[x.name,x]));
const bindings=firstRuntime.variables.engine_doors.scene_doors;
check(bindings.length===13,"the authored Lifeinvader leaves are configured");
for(const leaf of bindings)check(sceneNames.get(leaf.id)?.mesh?.asset?.endsWith(".ydr"),"binding references an original FirstFPS model");

const mockSource=`
export const views=[];
export class PlayerPresentation {configure(){} frame(...args){views.push(args);return [];} weaponFrame(){return [];} shutdown(){return [];} equipmentEntityId(){return "item";}}
export class FistPose {abandon(){} clear(){return [];}frame(){return [];}}
export class RecoilPose {reset(){}update(){}kick(){}sample(){return 0;}}
export const inventoryState=()=>({equipped:{}}),itemDefinition=()=>undefined,weaponReference=()=>null;
export const muzzleFlash=()=>({}),audioCommand=()=>null,playWeaponAnimation=()=>{};
`;
const mockUrl="data:text/javascript,"+encodeURIComponent(mockSource),mock=await import(mockUrl);
const presentationSource=stripTypeScriptTypes(readFileSync(root+"Shared/Content/scripts/newviso/gameplay/combat/presentation.ysc","utf8"),{mode:"transform"})
 .replace(/from '[^']+'/g,'from "'+mockUrl+'"');
const {CombatantPresentation}=await import("data:text/javascript,"+encodeURIComponent(presentationSource));
const presentationSettings={scene_id:"ambient",eye_height:1.65,first_person:{right:{weapon_joint:"hand"}}};
const measured={ready:true,eye:[2,1.6,3],forward:[.6,0,-.8]};
const runtime={peds:{physics:{bodies:[{actor_id:"ambient",entity_key:"700",center_offset:.9,weapon_pose:measured}]}},
 physics:{bodies:[{entity_key:"700",position:[2,.9,3]}]}};
const payload={runtime,delta_seconds:1/60,elapsed_seconds:1,project:{runtime:{variables:{}}}};
const ped={dead:false,profile:{},combat:{mode:"idle",last_seen_position:null}};
const presentation=new CombatantPresentation(presentationSettings);
for(const goal of [null,[2,0,8]]){
 ped.combat.last_seen_position=goal;
 presentation.frame(payload,{id:"ambient"},ped,{position:[2,0,3]},null);
 const direction=mock.views.at(-1)[4];
 check(direction.every(Number.isFinite),"idle ambient NPC without a shared player pose has a finite presentation");
 check(direction.every((v,i)=>Math.abs(v-measured.forward[i])<1e-6),"missing or rear targets retain the measured body forward");
}

console.log(JSON.stringify({passed:true,checks}));
