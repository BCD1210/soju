//! Read-only evaluation of the loaded executable's DAT requirement programs.
//! Unknown instructions fail closed. Command handlers still decide acceptance.
use bw_dat::{Game, Unit, UnitId, TechId, UpgradeId};
use serde_json::{json, Value};
use super::Runtime;

fn evaluate(mut word: impl FnMut(usize)->Option<u16>, level:u8, mut predicate:impl FnMut(u16,Option<u16>,Option<u16>)->bool)->bool {
    let mut pos=0;
    let mut group=false;
    loop {
        let Some(op)=word(pos) else {return false};pos+=1;
        if op==0xffff {return true}
        if op==0xff1f {
            let target=0xff1f+u16::from(level.min(2));
            if op!=target {
                loop {let Some(next)=word(pos) else {return false};pos+=1;if next==target {break}}
            }
            continue;
        }
        if op==0xff20 || op==0xff21 {return true}
        let mut arg=None;let mut count=None;
        if matches!(op,0xff02|0xff03|0xff04|0xff0f|0xff25|0xff40) {
            arg=word(pos);pos+=1;if arg.is_none(){return false}
        }
        if op==0xff40 {count=word(pos);pos+=1;if count.is_none(){return false}}
        group |= predicate(op,arg,count);
        if word(pos)==Some(0xff01) {pos+=1;continue}
        if !group {return false}group=false;
    }
}

unsafe fn allowed(r:&Runtime,game:Game,unit:Unit,table:u32,id:u16,level:u8)->bool {
    let Some(get)=r.requirements else {return false};
    let program=get(table,u32::from(id));
    if program.is_null() || unit.is_disabled() || !unit.is_completed() {return false}
    let player=unit.player();
    let mut hallucinations=false;
    let result=evaluate(|offset|if offset<256 {Some(*program.add(offset))}else{None},level,|op,arg,count|{
        let id=UnitId(arg.unwrap_or(op));
        match op {
            0..=227=>game.completed_count(player,id)>0,
            0xff02=>unit.matches_id(id),0xff03=>game.unit_count(player,id)>0,
            0xff04=>unit.addon().is_some_and(|addon|addon.is_completed() && addon.matches_id(id)),
            0xff05=>unit.is_landed_building(),0xff06=>unit.id().is_building() && !unit.is_landed_building(),
            0xff07=>unit.first_queued_unit().is_none() && !unit.is_building_addon() && unit.tech_in_progress().is_none() && unit.upgrade_in_progress().is_none(),
            0xff08=>!unit.is_building_addon(),0xff09=>unit.tech_in_progress().is_none(),0xff0a=>unit.upgrade_in_progress().is_none(),
            0xff0b=>!unit.is_constructing_building(),0xff0c=>unit.addon().is_none(),0xff0d=>unit.nydus_linked().is_none(),
            0xff0e=>unit.fighter_amount()+((0..5).filter(|&i|unit.nth_queued_unit(i).is_some()).count() as u32)<unit.hangar_cap(game),
            0xff0f|0xff25=>arg.is_some_and(|id|id<44 && game.tech_researched(player,TechId(id))),
            0xff10=>!unit.has_nuke(),0xff11=>!unit.is_burrowed(),0xff12=>!unit.is_landed_building(),0xff13=>unit.is_landed_building(),
            0xff14=>!unit.is_landed_building() && ![0,3].contains(&unit.id().rclick_action()) && !(unit.id().0==103 && unit.is_burrowed()),
            0xff15=>unit.id().ground_weapon().is_some() || unit.id().air_weapon().is_some() || unit.subunit_linked().is_some_and(|u|u.id().ground_weapon().is_some() || u.id().air_weapon().is_some()) || [72,83].contains(&unit.id().0),
            0xff16=>unit.id().is_worker(),0xff17=>unit.id().flags()&0x20!=0,0xff18=>unit.is_transport(game),
            0xff19=>unit.id().flags()&0x800!=0,0xff1a=>unit.id().is_subunit(),0xff1b=>unit.mine_amount(game)>0,
            0xff1d=>true,0xff1e=>{hallucinations=true;true},0xff24=>(**game).is_bw!=0,0xff26=>unit.is_burrowed(),
            0xff40=>arg.is_some_and(|id|id<228 && game.unit_count(player,UnitId(id))<u32::from(count.unwrap_or(0))),
            _=>false,
        }
    });
    result && (!unit.is_hallucination() || hallucinations)
}

pub unsafe fn query(r:&Runtime,game:Game,unit:Unit,requests:&[Value])->Value {
    let player=unit.player();
    let rows:Vec<_>=requests.iter().map(|request|{
        let op=request["op"].as_str().unwrap_or("");
        let id=request["type"].as_u64().and_then(|id|u16::try_from(id).ok());
        let mut minerals=0;let mut gas=0;let mut energy=0;
        let ready=match (op,id) {
            ("build"|"train"|"morph"|"addon",Some(id)) if id<228=>{
                let kind=UnitId(id);minerals=kind.mineral_cost();gas=kind.gas_cost();
                game.unit_available(player,kind) && allowed(r,game,unit,0,id,0)
            },
            ("research",Some(id)) if id<44=>{
                let tech=TechId(id);minerals=tech.mineral_cost();gas=tech.gas_cost();
                game.tech_available(player,tech) && !game.tech_researched(player,tech) && !game.tech_in_progress(player,tech) && allowed(r,game,unit,2,id,0)
            },
            ("upgrade",Some(id)) if id<61=>{
                let upgrade=UpgradeId(id);let level=game.upgrade_level(player,upgrade);
                minerals=upgrade.mineral_cost()+upgrade.mineral_factor()*u32::from(level);
                gas=upgrade.gas_cost()+upgrade.gas_factor()*u32::from(level);
                level<game.upgrade_max_level(player,upgrade) && !game.upgrade_in_progress(player,upgrade) && allowed(r,game,unit,1,id,level)
            },
            ("spell",_)=>super::spells::SPELLS.iter().find(|spell|Some(spell.action)==request["action"].as_str()).is_some_and(|spell|{
                spell.casters.contains(&unit.id().0) && spell.order.tech().is_some_and(|tech|{energy=tech.energy_cost();game.tech_researched(player,tech) && allowed(r,game,unit,3,tech.0,0)})
            }),
            ("ability",_)=>{
                let action=request["action"].as_str().unwrap_or("");
                let tech=match action {"stim"=>0,"siege"|"unsiege"=>5,"cloak"|"decloak"=>if unit.id().0==1 {10}else{9},"burrow"|"unburrow"=>11,_=>44};
                if action=="cloak" {energy=TechId(tech).energy_cost();}
                tech<44 && (action!="stim" || (**unit).flingy.hitpoints>10*256) && (unit.id().0==103 || game.tech_researched(player,TechId(tech))) && allowed(r,game,unit,3,tech,0)
            },
            ("load"|"unload",_)=>unit.is_completed() && !unit.is_disabled() && unit.is_transport(game) && (op!="unload" || unit.cargo_count()>0),
            _=>true,
        };
        json!({"payload":request,"requirementsMet":ready,"mineralCost":minerals,"gasCost":gas,"energyCost":energy})
    }).collect();
    json!({"ok":true,"frame":game.frame_count(),"unit":super::handle(*unit),"choices":rows})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(words:&[u16],level:u8)->bool {evaluate(|i|words.get(i).copied(),level,|op,_,_|op==1)}
    #[test] fn groups_are_anded_and_alternatives_ored(){assert!(check(&[0,0xff01,1,1,0xffff],0));assert!(!check(&[1,0,0xffff],0));}
    #[test] fn upgrade_branches_and_truncation(){assert!(check(&[0xff1f,0,0xffff,0xff20,1,0xffff,0xff21,0,0xffff],1));assert!(!check(&[0xff1f,1],2));assert!(!check(&[0xff02],0));}
}
