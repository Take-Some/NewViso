/** Physical recoil verification; needs only Node's built-in TypeScript stripper. */
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {stripTypeScriptTypes} from 'node:module';
import assert from 'node:assert/strict';
const source=fileURLToPath(new URL('../../Shared/Content/scripts/newviso/gameplay/weapons/recoil_pose.ysc',import.meta.url));
const {RecoilPose}=await import('data:text/javascript,'+encodeURIComponent(stripTypeScriptTypes(readFileSync(source,'utf8'))));
for(const hz of [30,60,144,240]){
 const spring=new RecoilPose();spring.kick(.04);let peak=spring.sample(),low=0;
 for(let i=0;i<hz;i++){const x=spring.update(1/hz);peak=Math.max(peak,x);low=Math.min(low,x);}
 assert(peak>.008&&peak<.088&&low>=-.0048,'impulse, limited recovery overshoot');assert(Math.abs(spring.sample())<1e-6,'returns to rest');
 for(let i=0;i<hz*3;i++){if(i%Math.max(1,Math.round(hz*.08))===0)spring.kick(.04);assert(Math.abs(spring.update(1/hz))<=.088,'bounded sustained fire');}
 spring.reset();assert.equal(spring.sample(),0);assert.equal(spring.update(.1),0);
}
const a=new RecoilPose(),b=new RecoilPose();a.kick(.03);b.kick(.03);
for(let i=0;i<6;i++)a.update(1/60);for(let i=0;i<24;i++)b.update(1/240);
assert(Math.abs(a.sample()-b.sample())<1e-10,'same physical response at different frame rates');
process.stdout.write('Recoil presentation PASS: impulse, return, burst bounds, reset, frame invariance\n');
