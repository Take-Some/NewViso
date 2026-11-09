/** Common door bindings, input arbitration and native-authoritative feedback. */
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import {fileURLToPath} from "node:url";
import {dirname,resolve} from "node:path";
import {stripTypeScriptTypes} from "node:module";
const root=fileURLToPath(new URL("../../",import.meta.url)),base=root+"Shared/Content/scripts/newviso/";
const cache=new Map(),events=[];
const mockEvents="export const Events={events:[],emit(topic,payload){this.events.push({topic,payload});}};";
async function load(path){
 path=resolve(path);if(cache.has(path))return cache.get(path);
 let code=path===resolve(base+"events.ysc")?mockEvents:readFileSync(path,"utf8");
 for(const m of [...code.matchAll(/from\s+["']([^"']+)["']/g)]){
  if(!m[1].startsWith("."))throw new Error("unexpected external script import");
  const mod=await load(resolve(dirname(path),m[1]));code=code.replace(m[0],"from "+JSON.stringify(mod.url));
 }
 const url="data:text/javascript,"+encodeURIComponent(stripTypeScriptTypes(code,{mode:"transform"}));
 const result={url,exports:await import(url)};cache.set(path,result);return result;
}
const {frameDoors,resetDoorControls}=(await load(base+"world/door_controls.ysc")).exports;
const {doorCommands,sceneDoorCommands,stopDoors}=(await load(base+"world/doors.ysc")).exports;
const {inventoryFrame}=(await load(base+"gameplay/items.ysc")).exports;
const {Events}=(await load(base+"events.ysc")).exports;
let checks=0,clock=0;const check=(x,msg)=>{assert(x,msg);checks++;};
const xml=readFileSync(root+"Shared/Content/config/engine/runtime.defaults.xml","utf8");
const section=xml.split("<engine_doors>")[1].split("</engine_doors>")[0],profile={};
for(const m of section.replace(/<mechanism>[\s\S]*?<\/mechanism>/,"").matchAll(/<([a-z_]+) value="([^"]+)"\s*\/>/g))profile[m[1]]=Number.isFinite(Number(m[2]))?Number(m[2]):m[2];
for(const m of section.matchAll(/<([a-z_]+) type="array">([\s\S]*?)<\/\1>/g))profile[m[1]]=[...m[2].matchAll(/value="([^"]+)"/g)].map(x=>Number(x[1]));
profile.mechanism={};
for(const m of section.split("<mechanism>")[1].split("</mechanism>")[0].matchAll(/<([a-z_]+) value="([^"]+)"\s*\/>/g)){
 const v=m[2];profile.mechanism[m[1]]=v==="true"?true:v==="false"?false:Number.isFinite(Number(v))?Number(v):v;
}
const spec={id:"any.scene.mesh",profile:"swing",anchor_entity:80,joint_ids:[81,82],mechanism:{latch:"handle",key_item:"key.office"},display_name:"Office door",navigation:false};
const commands=sceneDoorCommands(spec,{swing:profile}),def=commands.find(x=>x.op==="door.configure");
check(commands.length===2&&def.persistent_id===spec.id,"one native mechanism attaches to the original hinge");
check(def.mechanism.latch==="handle"&&def.mechanism.bolt_torque_nm>0,"hardware override preserves common finite physical limits");
check(def.navigation===false,"cabinet hardware need not create a passage");
check(!commands.some(c=>c.op==="physics.body.pose.set"),"binding never fakes an opening pose");
check(stopDoors().map(x=>x.op).join(",")==="door.remove,physics.hinge.unbind_scene","mechanism retires before hinge ownership");
const sample={id:"shared.textured.door",profile:"swing",hinge_position:[0,1.2,0],size:[1.3,2.4,.055],rotation_degrees:[0,0,0],hinge_side:-1,joint_ids:[91,92]};
const leaf=doorCommands(sample,{swing:profile}).find(c=>c.op==="scene.dynamic_entity.upsert"&&c.id===sample.id);
check(leaf.asset_ref==="models/props/doors/office_panel.ydr"&&leaf.visual==="none","common door uses the retained textured office model");
check(JSON.stringify(leaf.scale)===JSON.stringify(sample.size)&&JSON.stringify(leaf.base_color)==="[1,1,1,1]","model fits the physical panel without multiplying the source texture by a brown tint");
const mirrored=doorCommands({...sample,id:"shared.textured.right",hinge_side:1,joint_ids:[93,94]},{swing:profile}).find(c=>c.id==="shared.textured.right");
check(mirrored.asset_ref==="models/props/doors/office_panel_right.ydr","opposite hinge uses correctly oriented source handles and UVs");
stopDoors();
function payload(){
 return {elapsed_seconds:++clock,project:{runtime:{variables:{
  engine_player:{scene_id:"player.character",physics_entity:900},engine_items:{owner_id:"player"},engine_doors:{enabled:true,controls:{use_key:23,lock_modifier_key:60}}
 }}},frame:{platform:{focused:true,cursor_captured:true},camera_navigation_enabled:true,input:{state:{keys:{pressed:[23],down:[]}}}},
 runtime:{scene:{camera:{position:[0,1.6,1],target:[0,1.6,-1]}},player_camera:{head_position:[0,1.6,1]},
 physics:{bodies:[{entity:900,position:[0,.9,1]}],doors:{events:[],doors:[{
 entity:42,entity_key:"42",ready:true,display_name:"Office door",handle_position:[0,.9,0],angle:0,
 geometry:{normal:[0,0,1],center:[0,1,0],half:[.6,1,.03]},
 state:{locked:true},config:{...profile.mechanism,latch:"handle",key_item:"key.office",free_egress_side:-1}
 }]}},items:{definitions:[{id:"token",category:"misc",display_name:"Token"}],inventories:[{owner_id:"player",entries:[],equipped:{},capacity:18}],pickups:[]}}};
}
let p=payload(),result=frameDoors(p);
check(result.ui.bindings["engine.door.hint"].includes("KEY REQUIRED"),"outside without credential reports a locked door");
check(p.runtime.physics.doors.doors[0].state.locked,"client UI cannot unlock the authoritative native state");
p=payload();p.runtime.items.inventories[0].entries=[{item_id:"wrong.key",quantity:1}];
check(frameDoors(p).ui.bindings["engine.door.hint"].includes("KEY REQUIRED"),"a different real item is not a key");
p=payload();p.runtime.items.inventories[0].entries=[{item_id:"key.office",quantity:1}];p.frame.input.state.keys.down=[60];
result=frameDoors(p);check(result.commands[0].action==="lock_toggle","modifier produces an explicit lock transaction");
check(p.runtime.items.inventories[0].entries[0].quantity===1,"door interaction does not consume a reusable key");
p=payload();p.runtime.physics.doors.doors[0].entity="5644790730183409667";p.runtime.physics.doors.doors[0].entity_key="5644790730183409667";
result=frameDoors(p);check(result.commands[0].entity_key==="5644790730183409667"&&!Object.hasOwn(result.commands[0],"entity"),"64-bit native identities survive shared input without Number rounding");
p=payload();p.runtime.physics.bodies[0].position=[0,.9,-1];p.runtime.player_camera.head_position=[0,1.6,-1];
p.runtime.scene.camera={position:[0,1.6,-1],target:[0,1.6,1]};
check(frameDoors(p).ui.bindings["engine.door.hint"].includes("OPEN"),"authored free egress is directional");
p=payload();p.runtime.physics.doors.doors[0].state.locked=false;
p.runtime.items.pickups=[{id:"token.drop",item_id:"token",quantity:1,collected:false,position:[0,.05,-.2],collection_radius:2,requires_interact:true}];
result=frameDoors(p);const inventory=inventoryFrame(p,{position:[0,.05,1],captured:true});
check(result.commands.some(c=>c.op==="door.interact"),"aimed nearer hardware owns the use press");
check(!inventory.commands.some(c=>c.op==="items.pickup.collect"),"one press cannot also collect an item");
check(inventory.summary.hint.includes("DOOR")||inventory.summary.hint.includes("Office door"),"the same hint reaches shared and project inventory HUD adapters");
p=payload();p.runtime.items.pickups=[{id:"token.drop",item_id:"token",quantity:1,collected:false,position:[0,.05,.85],collection_radius:2,requires_interact:true}];
check(frameDoors(p).commands.length===0,"closer pickup keeps interaction priority");
check(inventoryFrame(p,{position:[0,.05,1],captured:true}).commands.some(c=>c.op==="items.pickup.collect"),"pickup remains available through the common controller");
p=payload();p.frame.platform.cursor_captured=false;check(frameDoors(p).commands.length===0,"released cursor does not operate doors");
p=payload();p.runtime.endorphin={protection:{actors:[{actor_id:"player.character",combat_reserved:true}]}};
check(frameDoors(p).commands.length===0,"protective hand authority blocks voluntary handle input");
p=payload();p.runtime.scene.camera.target=[0,1.6,3];check(frameDoors(p).commands.length===0,"use does not operate a door behind the view");
resetDoorControls();Events.events.length=0;p=payload();p.runtime.physics.doors.events=[{sequence:10,kind:"latched",actor_id:null}];
frameDoors(p);frameDoors(p);check(Events.events.length===1&&Events.events[0].topic==="door.latched","native state edge is dispatched once across repeated snapshots");
console.log(JSON.stringify({passed:true,checks}));
