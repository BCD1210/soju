"""Verify commander commands by before/after native state in private M15 fixtures."""
import argparse
import json
from pathlib import Path
import re
import time

from client import RUNTIME, request


def tables():
    source=(Path(__file__).parent/'CommandCard.swift').read_text()
    result={}
    for name in ('research','upgrades'):
        section=source.split(f'let {name}:',1)[1].split('\n        let ' if name=='research' else '\n        for ',1)[0]
        result[name]={int(producer):[(int(kind),label) for kind,label in re.findall(r'\((\d+),"([^"]+)"\)',items)]
                      for producer,items in re.findall(r'(\d+):\[([^\]]+)\]',section)}
    return result


class Verification:
    def __init__(self,race,phase="gameplay"):
        self.race=race
        self.folder=RUNTIME/'3d-proof/m15'/race/phase
        self.folder.mkdir(parents=True,exist_ok=True)
        self.results=[]
        self.initial=request()
        if f'Soju-M15-{race}-' not in self.initial['map']['path']:
            raise RuntimeError('Fresh private per-race fixture required')

    def unit(self,kind,state=None):
        state=state or request()
        return next((u for u in state['units'] if u['type']==kind and u['owner']==state['player']),None)

    def run(self,name,command,predicate,seconds=15):
        before=request();ack=None;after=before;error=None
        try:
            ack=request(**command)
            deadline=time.monotonic()+seconds
            while time.monotonic()<deadline:
                after=request()
                if after['session']!=before['session']:
                    raise RuntimeError('Game session changed')
                if after['frame']>before['frame'] and predicate(before,after):
                    break
                time.sleep(.1)
            else:
                raise RuntimeError('Native transition not observed')
        except (RuntimeError,TimeoutError) as failure:
            error=str(failure)
        file=self.folder/(name+'.json')
        file.write_text(json.dumps({'command':command,'ack':ack,'before':before,'after':after,'error':error},indent=2)+'\n')
        row={'race':self.race,'command':name,'verified':error is None,'evidence':str(file),'error':error}
        self.results.append(row)
        (self.folder/'results.json').write_text(json.dumps(self.results,indent=2)+'\n')
        print(json.dumps(row),flush=True)
        return error is None

    def supplemental(self):
        if self.race=='zerg':
            self.transport()
            hatchery=self.unit(131);uid=hatchery['id']
            if self.run('cancel-building-morph-start',{'op':'morph','units':[uid],'type':132},lambda b,a:any(u['id']==uid and not u['completed'] and 132 in u['buildQueue'] for u in a['units']),10):
                self.run('cancel-building-morph',{'op':'cancel','units':[uid],'action':'morph'},lambda b,a:a['minerals']>b['minerals'] and any(u['id']==uid and u['type']==131 and u['completed'] for u in a['units']),10)
            return
        if self.race=='protoss':
            caster=self.unit(63)
            self.run('feedback-scout',{'op':'move','units':[caster['id']],'x':46*32,'y':58*32},lambda b,a:any(u['type']==67 and u['owner']==1 for u in a['units']),30)
            state=request();target=next((u for u in state['units'] if u['type']==67 and u['owner']==1),None)
            if target is None:return
            self.run('spell-feedback',{'op':'spell','units':[caster['id']],'action':'feedback','target':target['id']},lambda b,a:any(u['id']==caster['id'] and u['energy']<next(v['energy'] for v in b['units'] if v['id']==caster['id'])-1 for u in a['units']),30)
            return
        state=request();target=next(u for u in state['units'] if u['type']==0 and u['owner']==1);caster=self.unit(34,state)
        self.run('spell-flare',{'op':'spell','units':[caster['id']],'action':'flare','target':target['id']},lambda b,a:any(u['id']==caster['id'] and u['energy']<next(v['energy'] for v in b['units'] if v['id']==caster['id'])-1 for u in a['units']),30)
        factory=self.unit(113);uid=factory['id']
        command={'op':'addon','units':[uid],'type':120,'tileX':int((factory['x']-64)//32)+4,'tileY':int((factory['y']-48)//32)+1}
        if self.run('cancel-addon-start',command,lambda b,a:any(u['id']==uid and u['constructingAddon'] for u in a['units']),30):
            self.run('cancel-addon',{'op':'cancel','units':[uid],'action':'addon'},lambda b,a:a['minerals']>b['minerals'] and any(u['id']==uid and not u['constructingAddon'] for u in a['units']),10)
        state=request();worker=self.unit(7,state)
        medics=[u for u in state['units'] if u['type']==34 and u['owner']==state['player']]
        medic=min(medics,key=lambda u:(u['x']-worker['x'])**2+(u['y']-worker['y'])**2)
        for other in medics:
            if other['id']!=medic['id']:request('move',units=[other['id']],x=3500,y=2500)
        vessel=self.unit(9,state)
        for unit in (medic,vessel):
            self.run('position-'+str(unit['type']),{'op':'move','units':[unit['id']],'x':worker['x']+64,'y':worker['y']+64},
                     lambda b,a,uid=unit['id']:any(u['id']==uid and (u['x']-worker['x'])**2+(u['y']-worker['y'])**2<128**2 for u in a['units']),30)
        self.run('restore-irradiate-setup',{'op':'spell','units':[vessel['id']],'action':'irradiate','target':worker['id']},
                 lambda b,a:any(u['id']==worker['id'] and u['status']['irradiate']>0 for u in a['units']),10)
        self.run('spell-restoration',{'op':'spell','units':[medic['id']],'action':'restoration','target':worker['id']},
                 lambda b,a:any(u['id']==worker['id'] and u['status']['irradiate']==0 for u in a['units']) and any(u['id']==medic['id'] and u['energy']<next(v['energy'] for v in b['units'] if v['id']==medic['id'])-1 for u in a['units']),10)
        marine=self.unit(0)
        self.run('heal-position',{'op':'move','units':[marine['id']],'x':worker['x']+96,'y':worker['y']+64},
                 lambda b,a:any(u['id']==marine['id'] and (u['x']-worker['x'])**2+(u['y']-worker['y'])**2<160**2 for u in a['units']),20)
        request('ability',units=[marine['id']],action='stim')
        self.run('spell-heal',{'op':'spell','units':[medic['id']],'action':'heal','target':marine['id']},
                 lambda b,a:any(u['id']==medic['id'] and u['energy']<next(v['energy'] for v in b['units'] if v['id']==medic['id'])-.125 for u in a['units']),10)

    def research(self):
        for operation,key in [('research','research'),('upgrade','upgrades')]:
            field='technologies' if operation=='research' else 'upgrades'
            producer=next((kind for kind in tables()[key] if self.unit(kind)),None)
            if producer is not None:
                unit=self.unit(producer);kind=tables()[key][producer][0][0]
                if self.run('cancel-'+operation+'-start',{'op':operation,'units':[unit['id']],'type':kind},lambda b,a:a[field][kind]['inProgress'],3):
                    self.run('cancel-'+operation,{'op':'cancel','units':[unit['id']],'action':operation},lambda b,a:not a[field][kind]['inProgress'] and a['minerals']>b['minerals'],3)
            seen=set()
            for producer,entries in tables()[key].items():
                unit=self.unit(producer)
                if not unit:
                    continue
                for kind,label in entries:
                    if kind in seen:
                        continue
                    seen.add(kind)
                    def changed(before,after,kind=kind,operation=operation):
                        field='technologies' if operation=='research' else 'upgrades'
                        old=before[field][kind];new=after[field][kind]
                        return (not old['researched'] and new['researched']) if operation=='research' else new['level']>old['level']
                    self.run(f'{operation}-{kind}',{'op':operation,'units':[unit['id']],'type':kind},changed,8)

    def transport(self):
        kind,passenger={'terran':(11,0),'zerg':(42,37),'protoss':(69,65)}[self.race]
        state=request();cargo=self.unit(passenger,state)
        candidates=[u for u in state['units'] if u['type']==kind and u['owner']==state['player']]
        transport=min(candidates,key=lambda u:(u['x']-cargo['x'])**2+(u['y']-cargo['y'])**2,default=None) if cargo else None
        if not transport or not cargo:
            return
        tid=transport['id']
        def count(state):
            return next(u['cargoCount'] for u in state['units'] if u['id']==tid)
        self.run('load',{'op':'load','units':[tid],'target':cargo['id']},lambda b,a:count(a)>count(b),30)
        self.run('unload',{'op':'unload','units':[tid]},lambda b,a:count(b)>0 and count(a)==0,20)

    def rally(self):
        base=self.unit({'terran':106,'zerg':131,'protoss':154}[self.race])
        if base:
            self.run('rally-ground',{'op':'rally','units':[base['id']],'x':512,'y':1536},lambda b,a:any(u['id']==base['id'] and u['rally']['x']==512 and u['rally']['y']==1536 for u in a['units']))

            worker=self.unit({'terran':7,'zerg':41,'protoss':64}[self.race])
            if worker:
                self.run('rally-unit',{'op':'rally','units':[base['id']],'target':worker['id']},lambda b,a:any(u['id']==base['id'] and u['rally']['target']==worker['id'] for u in a['units']))

    def cancellations(self):
        producer,kind={'terran':(106,7),'zerg':(35,41),'protoss':(154,64)}[self.race]
        unit=self.unit(producer)
        if unit:
            uid=unit['id'];op='morph' if self.race=='zerg' else 'train'
            if self.run('cancel-'+op+'-start',{'op':op,'units':[uid],'type':kind},
                        lambda b,a:any(u['id']==uid and (u['type']==36 if op=='morph' else kind in u['buildQueue']) for u in a['units'])):
                self.run('cancel-'+op,{'op':'cancel','units':[uid],'action':op},
                         lambda b,a:a['minerals']>b['minerals'] and not any(u['id']==uid and (u['type']==36 if op=='morph' else kind in u['buildQueue']) for u in a['units']))
        worker,building={'terran':(7,109),'zerg':(41,143),'protoss':(64,156)}[self.race]
        unit=self.unit(worker)
        if unit:
            x,y=(18,44) if self.race=='zerg' else (18,56)
            before=request();old={u['id'] for u in before['units']}
            if self.run('cancel-build-start',{'op':'build','units':[unit['id']],'type':building,'tileX':x,'tileY':y},
                        lambda b,a:any(u['type']==building and not u['completed'] for u in a['units']),50):
                unfinished=next(u for u in request()['units'] if u['type']==building and not u['completed'])
                self.run('cancel-build',{'op':'cancel','units':[unfinished['id']],'action':'construction'},
                         lambda b,a:a['minerals']>b['minerals'] and not any(u['id']==unfinished['id'] and u['type']==building for u in a['units']))

    def spells(self):
        metadata=request('unit_types')
        for spell in metadata['spells']:
            available=request();casters=[u for u in available['units'] if u['type'] in spell['casters'] and u['owner']==available['player']]
            caster=max(casters,key=lambda u:u['energy'],default=None)
            if not caster:
                continue
            action=spell['action'];state=request()
            command={'op':'spell','units':[caster['id']],'action':action}
            if spell['target']=='unit':
                preferred={'lockdown':5,'irradiate':0,'matrix':0,'heal':0,'restoration':0,'flare':0,'consume':37,'parasite':0,'broodling':0,'hallucination':65,'mind_control':5,'feedback':67,'yamato':5}.get(action,0)
                candidates=[u for u in state['units'] if u['type']==preferred and u['id']!=caster['id']]
                own=action in ['lockdown','irradiate','matrix','heal','restoration','consume','hallucination']
                target=next((u for u in candidates if (u['owner']==state['player'])==own),None)
                if not target:
                    continue
                command['target']=target['id']
            else:
                command.update(x=int(caster['x'])+64,y=int(caster['y'])+64)
            if action=='heal' and command.get('target'):
                request('ability',units=[command['target']],action='stim')
            cid=caster['id']
            def changed(before,after,cid=cid,action=action,command=command):
                old=next(u for u in before['units'] if u['id']==cid)
                new=next((u for u in after['units'] if u['id']==cid),None)
                if action=='mine':
                    return sum(u['type']==13 for u in after['units'])>sum(u['type']==13 for u in before['units'])
                if action=='consume':
                    return not any(u['id']==command['target'] for u in after['units'])
                return new is not None and new['energy']<old['energy']-1
            self.run('spell-'+action,command,changed,15)

    def production(self):
        if self.race=='zerg':
            spire=self.unit(141)
            if spire:
                self.run('morph-141-137',{'op':'morph','units':[spire['id']],'type':137},lambda b,a:any(u['id']==spire['id'] and u['type']==137 and u['completed'] for u in a['units']),150)
            operations=[(35,41),(35,37),(35,38),(35,43),(35,45),(35,46),(35,42),(35,39),(35,47),
                        (38,103),(43,44),(43,62),(131,132),(132,133),(143,144),(143,146)]
            pending=[]
            used=set()
            initial=request()
            for producer,kind in operations:
                unit=next((u for u in initial['units'] if u['type']==producer and u['owner']==initial['player'] and u['id'] not in used),None)
                if not unit:
                    continue
                used.add(unit['id']);before=request();command={'op':'morph','units':[unit['id']],'type':kind}
                ack=request(**command);pending.append((producer,kind,unit,before,command,ack))
            deadline=time.monotonic()+180
            while pending and time.monotonic()<deadline:
                after=request()
                for entry in pending[:]:
                    producer,kind,unit,before,command,ack=entry
                    produced=[u for u in after['units'] if u['owner']==after['player'] and u['type']==kind and u['completed']]
                    old_ids={u['id'] for u in before['units'] if u['type']==kind}
                    if any(u['id']==unit['id'] or u['id'] not in old_ids for u in produced):
                        file=self.folder/f'morph-{producer}-{kind}.json'
                        file.write_text(json.dumps({'before':before,'after':after,'command':command,'ack':ack},indent=2))
                        row={'race':self.race,'command':f'morph-{producer}-{kind}','verified':True,'evidence':str(file)}
                        self.results.append(row);print(json.dumps(row),flush=True);pending.remove(entry)
                time.sleep(.25)
            for producer,kind,unit,before,command,ack in pending:
                file=self.folder/f'morph-{producer}-{kind}.json'
                file.write_text(json.dumps({'before':before,'after':request(),'command':command,'ack':ack},indent=2))
                self.results.append({'race':self.race,'command':f'morph-{producer}-{kind}','verified':False,'evidence':str(file)})
            (self.folder/'results.json').write_text(json.dumps(self.results,indent=2))
        if self.race=='terran':
            building=self.unit(106)
            if building:
                bid=building['id']
                self.run('lift',{'op':'lift','units':[bid]},lambda b,a:any(u['id']==bid and u['airborne'] for u in a['units']))
                self.run('land',{'op':'land','units':[bid],'tileX':14,'tileY':15},lambda b,a:any(u['id']==bid and not u['airborne'] for u in a['units']),30)
            for producer,kind in [(106,107),(106,108),(113,120),(114,115),(116,117),(116,118)]:
                state=request();unit=next((u for u in state['units'] if u['type']==producer and u['owner']==state['player'] and not u.get('addon')),None)
                if not unit:continue
                self.run(f'addon-{kind}',{'op':'addon','units':[unit['id']],'type':kind,'tileX':int((unit['x']-unit.get('footprintWidth',128)/2)//32)+4,'tileY':int((unit['y']-unit.get('footprintHeight',96)/2)//32)+1},
                         lambda b,a,uid=unit['id'],kind=kind:any(u['id']==uid and any(child['id']==u.get('addon') and child['type']==kind and child['completed'] for child in a['units']) for u in a['units']),90)

    def modes(self):
        rows={'terran':[(0,'stim','status','stim'),(5,'siege','type',30),(30,'unsiege','type',5),(8,'cloak','cloaked',True),(8,'decloak','cloaked',False)],
              'zerg':[(37,'burrow','burrowed',True),(37,'unburrow','burrowed',False)],'protoss':[]}[self.race]
        for kind,action,field,expected in rows:
            unit=self.unit(kind)
            if not unit:
                continue
            def changed(before,after,unit=unit,field=field,expected=expected):
                target=next((u for u in after['units'] if u['id']==unit['id']),None)
                if not target:return False
                return target[field][expected]>0 if field=='status' else target[field]==expected
            self.run(action,{'op':'ability','units':[unit['id']],'action':action},changed)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--race',choices=['terran','zerg','protoss'],required=True)
    parser.add_argument('--research',action='store_true')
    parser.add_argument('--phase')
    parser.add_argument('--supplemental',action='store_true')
    args=parser.parse_args();verification=Verification(args.race,args.phase or ("research" if args.research else "gameplay"))
    if args.supplemental:verification.supplemental()
    elif args.research:verification.research()
    else:verification.rally();verification.transport();verification.modes();verification.cancellations();verification.spells();verification.production()
