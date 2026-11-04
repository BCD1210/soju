import * as THREE from './three.module.js';
const $=id=>document.getElementById(id), scale=16;
const scene=new THREE.Scene();scene.background=new THREE.Color(0x13242d);scene.fog=new THREE.Fog(0x13242d,60,170);
const renderer=new THREE.WebGLRenderer({antialias:true});renderer.setPixelRatio(Math.min(devicePixelRatio,2));renderer.setSize(innerWidth,innerHeight);
renderer.xr.enabled=true;renderer.xr.setReferenceSpaceType('local-floor');document.body.prepend(renderer.domElement);
const camera=new THREE.PerspectiveCamera(72,innerWidth/innerHeight,.08,400),rig=new THREE.Group();rig.add(camera);scene.add(rig);
scene.add(new THREE.HemisphereLight(0xbcecff,0x273b32,2.5));
const sun=new THREE.DirectionalLight(0xffddab,2.3);sun.position.set(-30,70,20);scene.add(sun);
const units=new Map(),ray=new THREE.Raycaster(),mouse=new THREE.Vector2(),keys=new Set();
let meta=null,state=null,selected=-1,terrain=null,yaw=.65,pitch=-.13,tactical=false,build=0,seq=0,lastResult=0,lastMove=0,lastSeen=0,drag=null;
let xrMode=0;const xrModes=['선택 / 명령','서플라이 건설','배럭 건설','생산','따라와','정지'];
const hudCanvas=document.createElement('canvas');hudCanvas.width=1024;hudCanvas.height=256;
const hudTexture=new THREE.CanvasTexture(hudCanvas),hud=new THREE.Mesh(new THREE.PlaneGeometry(.75,.1875),new THREE.MeshBasicMaterial({map:hudTexture,transparent:true,depthTest:false}));
hud.position.set(0,-.32,-1);hud.renderOrder=999;camera.add(hud);hud.visible=false;
function height(x,y){if(!meta)return 0;const tx=Math.max(0,Math.min(meta.width-1,Math.floor(x/32))),ty=Math.max(0,Math.min(meta.height-1,Math.floor(y/32)));return meta.terrain[ty*meta.width+tx]*.45;}
function message(s){$('message').textContent=s;}
function find(id){return state?.units.find(u=>u.id===id);}
async function command(op,extra={},id=selected){
 if(!state||performance.now()-lastSeen>1500){message('게임 연결이 끊어졌습니다.');return;}
 const r=await fetch('/command',{method:'POST',headers:{'Content-Type':'application/json','X-Soju-Token':window.SOJU_TOKEN},body:JSON.stringify({id:++seq,op,unit:id,...extra})});
 if(!r.ok)message('명령을 전달하지 못했습니다.');
}
function makeTerrain(m){
 const g=new THREE.BoxGeometry(2,1,2),mat=new THREE.MeshStandardMaterial({color:0x596155,roughness:1});
 terrain=new THREE.InstancedMesh(g,mat,m.width*m.height);const transform=new THREE.Object3D();
 for(let y=0;y<m.height;y++)for(let x=0;x<m.width;x++){const i=y*m.width+x,top=m.terrain[i]*.45;
 transform.position.set(x*2+1,top/2-.12,y*2+1);transform.scale.set(1,Math.max(.24,top+.24),1);transform.updateMatrix();terrain.setMatrixAt(i,transform.matrix);
 terrain.setColorAt(i,new THREE.Color().setHSL(.20,.09,.24+((x*7+y*13)%7)*.009+(m.terrain[i]%2)*.04));}
 terrain.instanceMatrix.needsUpdate=true;scene.add(terrain);
}
function material(color){return new THREE.MeshStandardMaterial({color,roughness:.72,metalness:.25});}
function shape(g,geometry,mat,x,y,z){const m=new THREE.Mesh(geometry,mat);m.position.set(x,y,z);g.add(m);return m;}
function makeUnit(u){
 const g=new THREE.Group();g.userData.unit=u.id;
 const color=u.mineral?0x65caff:u.owner===meta.player?0x53a1c6:u.owner<0?0x657d65:0xb76474;
 const body=material(color),dark=material(0x22313c);
 if(u.mineral){for(let i=0;i<3;i++){const c=shape(g,new THREE.OctahedronGeometry(.6),body,(i-1)*.5,.6+(i%2)*.3,0);c.scale.y=1.8;c.rotation.z=(i-1)*.3;}}
 else if(u.building){
 const w=u.tileWidth*2*.92,d=u.tileHeight*2*.92,h=u.type===106?3.2:u.type===111?2.8:1.3;
 shape(g,new THREE.BoxGeometry(w,h,d),body,0,h/2,0);
 shape(g,new THREE.BoxGeometry(w*.68,.35,d*.7),dark,0,h+.17,0);
 shape(g,new THREE.BoxGeometry(w*.25,h*.5,.12),material(0x67eddb),0,h*.4,d/2+.03);
 }else{
 shape(g,new THREE.BoxGeometry(u.worker?1.3:.8,u.worker?1:.85,.65),body,0,1,0);
 shape(g,new THREE.BoxGeometry(.54,.48,.54),dark,0,1.65,0);
 for(const x of [-.28,.28])shape(g,new THREE.BoxGeometry(.24,.6,.3),body,x,.3,0);
 shape(g,new THREE.BoxGeometry(.18,.2,.9),dark,.52,1.08,-.35);
 }
 const ring=new THREE.Mesh(new THREE.TorusGeometry(Math.max(.7,u.tileWidth*.8),.045,6,32),new THREE.MeshBasicMaterial({color:0x89ffce}));
 ring.rotation.x=-Math.PI/2;ring.position.y=.08;ring.name='selection';g.add(ring);scene.add(g);return g;
}
function sync(s){
 state=s;const alive=new Set();
 for(const u of s.units){alive.add(u.id);let m=units.get(u.id);if(!m){m=makeUnit(u);units.set(u.id,m);}
 m.position.set(u.x/scale,height(u.x,u.y),u.y/scale);
 if(!u.building)m.rotation.y=-u.angle-Math.PI/2;
 m.getObjectByName('selection').visible=u.id===selected;
 m.visible=u.id!==s.commander||tactical;
 m.traverse(o=>{if(o.isMesh&&o.name!=='selection'){o.material.opacity=u.completed?1:.5;o.material.transparent=!u.completed;}});
 }
 for(const [id,m] of units)if(!alive.has(id)){scene.remove(m);m.traverse(o=>{o.geometry?.dispose();o.material?.dispose();});units.delete(id);}
 const u=find(selected);$('selected').textContent=u?u.name.replaceAll('_',' ')+' · HP '+u.hp+'/'+u.maxHp:'유닛을 선택하세요';
 $('stats').textContent='◆ '+s.minerals+'   ◈ '+s.gas+'   인구 '+s.supplyUsed/2+'/'+s.supplyTotal/2;
 const ctx=hudCanvas.getContext('2d');ctx.clearRect(0,0,1024,256);ctx.fillStyle='#071820de';ctx.fillRect(0,0,1024,256);
 ctx.fillStyle='#b8fff1';ctx.font='40px sans-serif';ctx.fillText('Minerals '+s.minerals+' / Supply '+s.supplyUsed/2+'/'+s.supplyTotal/2,24,60);
 ctx.fillStyle='white';ctx.font='30px sans-serif';ctx.fillText(xrModes[xrMode]+' | '+(u?u.name:'유닛 선택'),24,120);
 ctx.font='25px sans-serif';ctx.fillText('트리거: 실행 · 그립: 모드 변경 · 스틱: 지휘관 이동',24,180);hudTexture.needsUpdate=true;
}
async function poll(){
 try{const r=await fetch('/state');if(!r.ok)throw Error('connection');const data=await r.json();
 if(data.error){message(data.error);return;}
 if(data.map&&!meta){meta=data.map;makeTerrain(meta);}
 if(data.state&&meta){lastSeen=performance.now();sync(data.state);}
 for(const v of data.results){seq=Math.max(seq,v.id);if(v.id>lastResult){lastResult=v.id;if(!v.ok)message(v.error||'명령 거부');}}
 }catch(e){message('게임 서버 연결을 확인하세요.');}
 finally{setTimeout(poll,120);}
}
function select(id){selected=id;build=0;message(find(id)?.name.replaceAll('_',' ')||'유닛 선택');}
function hits(r){return r.intersectObjects([...units.values()].filter(m=>m.visible),true);}
function unitAtHit(hit){let o=hit?.object;while(o&&o.userData.unit===undefined)o=o.parent;return o?find(o.userData.unit):null;}
function pointAt(r){const h=terrain?r.intersectObject(terrain)[0]:null;return h?{x:Math.floor(h.point.x*scale),y:Math.floor(h.point.z*scale)}:null;}
function act(r,right=false,xr=false){
 const target=unitAtHit(hits(r)[0]),u=find(selected),point=pointAt(r);
 const mode=xr?xrMode:0;
 if(mode===3){train();return;}if(mode===4){command('follow',{target:state.commander});return;}if(mode===5){command('stop');return;}
 const type=build||(mode===1?109:mode===2?111:0);
 if(type&&point){command('build',{type,...point});build=0;if(xr)xrMode=0;message('건설 명령 전달');return;}
 if(target?.owner===meta.player&&!right){select(target.id);return;}
 if(!u)return;
 if(target?.mineral&&u.worker){command('gather',{target:target.id});message('자원 채취 명령 전달');}
 else if(target&&target.owner>=0&&target.owner!==meta.player){command('attack',{target:target.id});message('공격 명령 전달');}
 else if(point&&(right||xr)){command('move',point);message('이동 명령 전달');}
}
function screenRay(e){mouse.set(e.clientX/innerWidth*2-1,1-e.clientY/innerHeight*2);ray.setFromCamera(mouse,camera);return ray;}
renderer.domElement.addEventListener('contextmenu',e=>e.preventDefault());
renderer.domElement.addEventListener('pointerdown',e=>{drag={x:e.clientX,y:e.clientY,lastX:e.clientX,lastY:e.clientY,button:e.button,moved:0};});
addEventListener('pointermove',e=>{if(!drag)return;const dx=e.clientX-drag.lastX,dy=e.clientY-drag.lastY;drag.moved+=Math.abs(dx)+Math.abs(dy);
 if(drag.button===0&&!tactical){yaw-=dx*.004;pitch=Math.max(-1.2,Math.min(1.2,pitch-dy*.004));}drag.lastX=e.clientX;drag.lastY=e.clientY;});
addEventListener('pointerup',e=>{if(drag&&drag.moved<6&&e.target===renderer.domElement&&meta)act(screenRay(e),drag.button===2);drag=null;});
addEventListener('keydown',e=>{if(['KeyW','KeyA','KeyS','KeyD'].includes(e.code)){keys.add(e.code);e.preventDefault();}});
addEventListener('keyup',e=>keys.delete(e.code));addEventListener('blur',()=>keys.clear());
$('view').onclick=()=>{tactical=!tactical;$('view').textContent=tactical?'지휘관 시점':'전술 시점';};
$('follow').onclick=()=>command('follow',{target:state.commander});$('stop').onclick=()=>command('stop');
function place(type){if(!find(selected)?.worker){message('SCV를 먼저 선택하세요.');return;}build=type;message('건설할 지면을 클릭하세요.');}
$('depot').onclick=()=>place(109);$('barracks').onclick=()=>place(111);
function train(){const u=find(selected);if(!u)return;command('train',{type:u.type===106?7:0});}
$('train').onclick=train;
for(let i=0;i<2;i++){
 const c=renderer.xr.getController(i);rig.add(c);
 const line=new THREE.Line(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(),new THREE.Vector3(0,0,-20)]),new THREE.LineBasicMaterial({color:0x83ffd8}));c.add(line);
 c.addEventListener('selectstart',()=>{c.updateWorldMatrix(true,false);ray.ray.origin.setFromMatrixPosition(c.matrixWorld);ray.ray.direction.set(0,0,-1).transformDirection(c.matrixWorld);if(meta)act(ray,false,true);});
 c.addEventListener('squeezestart',()=>{xrMode=(xrMode+1)%xrModes.length;});
}
async function setupXR(){
 const supported=!!navigator.xr&&await navigator.xr.isSessionSupported('immersive-vr');
 $('vr').disabled=!supported;$('vr').textContent=supported?'VR 시작':'이 브라우저에서 VR 사용 불가';
 $('vr').onclick=async()=>{try{const session=await navigator.xr.requestSession('immersive-vr',{requiredFeatures:['local-floor']});tactical=false;camera.rotation.set(0,0,0);camera.position.set(0,0,0);await renderer.xr.setSession(session);}catch(e){message(e.message);}};
}
setupXR().catch(e=>{$('vr').textContent='VR 확인 실패';message(e.message);});
renderer.setAnimationLoop(time=>{
 const hero=find(state?.commander);const xr=renderer.xr.isPresenting;hud.visible=xr;
 if(hero){
 const x=hero.x/scale,z=hero.y/scale,h=height(hero.x,hero.y);
 rig.position.set(x,h,z);
 if(!xr){camera.position.set(tactical?12:0,tactical?32:1.8,tactical?24:0);if(tactical)camera.lookAt(x,h,z);else camera.rotation.set(pitch,yaw,0,'YXZ');}
 let dx=(keys.has('KeyD')?1:0)-(keys.has('KeyA')?1:0),dy=(keys.has('KeyS')?1:0)-(keys.has('KeyW')?1:0),angle=yaw;
 if(xr){const sess=renderer.xr.getSession();for(const source of sess.inputSources){if(source.gamepad?.axes.length>=4){dx+=source.gamepad.axes[2];dy+=source.gamepad.axes[3];}}
 const direction=new THREE.Vector3();renderer.xr.getCamera().getWorldDirection(direction);angle=Math.atan2(-direction.x,-direction.z);}
 if(time-lastMove>180&&Math.hypot(dx,dy)>.25&&performance.now()-lastSeen<1500){
 const norm=Math.max(1,Math.hypot(dx,dy));dx/=norm;dy/=norm;
 const xx=hero.x+(Math.cos(angle)*dx+Math.sin(angle)*dy)*64,yy=hero.y+(-Math.sin(angle)*dx+Math.cos(angle)*dy)*64;
 if(xx>=0&&yy>=0&&xx<meta.width*32&&yy<meta.height*32)command('move',{x:xx,y:yy},hero.id);
 lastMove=time;
 }
 }
 renderer.render(scene,camera);
});
addEventListener('resize',()=>{camera.aspect=innerWidth/innerHeight;camera.updateProjectionMatrix();renderer.setSize(innerWidth,innerHeight);});
poll();
