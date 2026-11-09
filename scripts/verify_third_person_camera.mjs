/** Third-person intent, follow and optical-effect invariants on the real Shared graph. */
import assert from "node:assert/strict";
import {readFileSync,writeFileSync,mkdirSync} from "node:fs";
import {fileURLToPath,pathToFileURL} from "node:url";
import {resolve,dirname,relative} from "node:path";
import {stripTypeScriptTypes} from "node:module";
const root=fileURLToPath(new URL("../../",import.meta.url));
const source=resolve(root,"Shared/Content/scripts/newviso");
const out=resolve(root,"NewViso/.newviso/cache/third-person-camera-work/js-modules");
mkdirSync(out,{recursive:true});
const built=new Map();
const events='let next=0;const handlers=new Map();export const Events={on(t,f){const id=++next;handlers.set(id,{t,f});return id;},off(id){handlers.delete(id);},emit(topic,payload){for(const h of handlers.values())if(h.t===topic)h.f({topic,payload});return {defaultPrevented:false};}};';
function moduleFile(file){
 file=resolve(file);if(built.has(file))return built.get(file);
 const dest=resolve(out,relative(source,file).replace(/\.ysc$/,".mjs"));
 mkdirSync(dirname(dest),{recursive:true});built.set(file,dest);
 let text=file===resolve(source,"events.ysc")?events:readFileSync(file,"utf8");
 text=text.replace("{ Vec3, CameraMode,","{ type Vec3, type CameraMode,");
 text=text.replace(/(from\s*["'])(\.[^"']+)(["'])/g,(_m,a,s,b)=>a+pathToFileURL(moduleFile(resolve(dirname(file),s))).href+b);
 writeFileSync(dest,stripTypeScriptTypes(text,{mode:"transform"}));return dest;
}
const {GameplayCamera}=await import(pathToFileURL(moduleFile(resolve(source,"gameplay/gameplay_camera.ysc"))));
const {playerCameraCommand}=await import(pathToFileURL(moduleFile(resolve(source,"gameplay/player_camera.ysc"))));
const {WEAPON_CAMERA_REFERENCE:Ref}=await import(pathToFileURL(moduleFile(resolve(source,"gameplay/weapon_camera_reference.ysc"))));
const {cameraMetadata}=await import(pathToFileURL(moduleFile(resolve(source,"gameplay/camera_dynamics.ysc"))));
const {Events}=await import(pathToFileURL(moduleFile(resolve(source,"events.ysc"))));
const settings={scene_id:"arbitrary.skeleton",eye_height:1.7,root_lift:0,first_person:{eye_joint:"any.head",eye_offset:[0,.08,-.08]},
 camera:{third_person_distance:3.25,third_person_min_distance:1.2,third_person_max_distance:8,third_person_shoulder:.48,third_person_height:.32,zoom_step:.45,collision_radius:.22,fov_y_degrees:72}};
const input={eye:[0,1.7,0],yaw:.4,pitch:.2,mode:"third_person",distance:3.25,physics_entity:900000000,
 actor_id:"arbitrary.skeleton",aiming:false,armed:true,moving:true};
let checks=0;const check=(v,m)=>{assert(v,m);checks++;};
const near=(a,b,e=1e-8)=>a.every((v,i)=>Math.abs(v-b[i])<e);
const angle=v=>Math.atan2(v[0],-v[2]);
const wrap=v=>Math.atan2(Math.sin(v),Math.cos(v));
const c=new GameplayCamera();const before=c.begin({elapsed_seconds:0,delta_seconds:1/60},settings,input);
c.recoil({Name:"test",RecoilShakeAmplitude:0},1,{recoil_pitch_radians:.04,recoil_yaw_radians:.03});
const after=c.sample();
check(near(after.position,before.position),"third-person recoil must not orbit or translate the camera boom");
check(Math.abs(wrap(angle(after.direction)-input.yaw))<1e-9,"automatic third-person recoil must not change heading");
check(after.direction[1]>before.direction[1],"vertical recoil remains visible");
check(after.command.collision.preserve_direction===true,"directional cameras request native collision orientation preservation");
c.shutdown();
for(const hz of [30,60,120,240]){
 const camera=new GameplayCamera();let elapsed=0;
 for(let i=0;i<hz*2;i++){
  elapsed+=1/hz;
  const sample={...input,eye:[i/hz,1.7+.1*Math.sin(i/hz),-i/hz],yaw:Math.PI-.05+i/hz*.25,pitch:.18*Math.sin(i/hz)};
  const payload={elapsed_seconds:elapsed,delta_seconds:1/hz,runtime:{player_camera:{available:true,animation_camera_angles_degrees:[40*Math.sin(i),179*Math.cos(i),60*Math.sin(i*.3)]}}};
  camera.begin(payload,settings,sample);
  if(i%7===0)camera.recoil({Name:"test",RecoilShakeAmplitude:0},1,{recoil_pitch_radians:.03,recoil_yaw_radians:.04});
  if(i===2)Events.emit("game.camera.shake",{actor_id:sample.actor_id,ref:cameraMetadata(Ref.fps).VehicleImpactShakeRef,amplitude:2});
  const frame=camera.sample(),boom=playerCameraCommand(sample.eye,sample.yaw,sample.pitch,"third_person",sample.distance,settings.camera,sample.physics_entity);
  check(near(frame.position,boom.position),"intent-only follow boom across animation, impacts and yaw wrap");
  check(Math.abs(wrap(angle(frame.direction)-sample.yaw))<1e-8,"no unsolicited left/right heading");
  const right=[Math.cos(sample.yaw),0,Math.sin(sample.yaw)];
  check(Math.abs(frame.up.reduce((s,v,j)=>s+v*right[j],0))<1e-8,"third person has no automatic roll");
  check(frame.position.every(Number.isFinite)&&frame.direction.every(Number.isFinite),"finite camera pose");
 }
 camera.shutdown();
}
const first=new GameplayCamera(),fp={...input,mode:"first_person"};
const a=first.begin({elapsed_seconds:0,delta_seconds:1/60},settings,fp);
first.recoil({Name:"test",RecoilShakeAmplitude:0},1,{recoil_pitch_radians:.04,recoil_yaw_radians:.03});
check(Math.abs(wrap(angle(first.sample().direction)-input.yaw))>1e-6,"first-person authored horizontal recoil remains active");
first.shutdown();
const custom=new GameplayCamera(),customSettings={...settings,camera:{...settings.camera,third_person_effects:{pitch_scale:1,heading_scale:1,roll_scale:0,translation_scale:0}}};
custom.begin({elapsed_seconds:0,delta_seconds:1/60},customSettings,input);
custom.recoil({Name:"test",RecoilShakeAmplitude:0},1,{recoil_yaw_radians:.03});
check(Math.abs(wrap(angle(custom.sample().direction)-input.yaw))>1e-6,"effect gains are configurable");
custom.shutdown();
console.log(JSON.stringify({passed:true,checks}));
