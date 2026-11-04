import Foundation

enum CommandTarget { case none, ground, unit, rally }
struct CommandChoice {
    let title:String,payload:Obj,target:CommandTarget
    var hotkey="",mineralCost=0.0,gasCost=0.0,energyCost=0.0,supplyCost=0.0,race=0
    var requirementReason:String?
    var priced=true
    var label:String {
        let cost=["research","upgrade"].contains(payload["op"] as? String ?? "") && priced ? " \(Int(mineralCost))/\(Int(gasCost))":""
        return (hotkey.isEmpty ? title : "[\(hotkey)] \(title)")+cost
    }
    func unavailable(_ state:Obj,_ unit:Obj)->String? {
        if let reason=requirementReason {return reason}
        if !priced {return "Loading costs"}
        if number(state,"minerals")<mineralCost {return "Not enough minerals"}
        if number(state,"gas")<gasCost {return "Not enough gas"}
        if ["spell","ability"].contains(payload["op"] as? String ?? "") && number(unit,"spellCooldown")>0 {return "Ability cooldown"}
        if number(unit,"energy")<energyCost {return "Not enough energy"}
        if supplyCost>0,let supply=(state["supplies"] as? [Obj])?.first(where:{Int(number($0,"race"))==race}),number(supply,"used")+supplyCost>min(400,number(supply,"provided")) {return "Not enough supply"}
        return nil
    }
    init(_ title:String,_ payload:Obj,_ target:CommandTarget = .none) {
        self.title=title;self.payload=payload;self.target=target
    }
}
// Producer relationships are commander rules; unit costs come from native units.dat.
// The original command handler remains authoritative for prerequisites and availability.
final class CommandCard {
    var costs=[Int:Obj]()
    var availability:Obj=[:]
    func acceptAvailability(_ response:Obj) {availability=response}
    func choiceKey(_ payload:Obj)->String {"\(payload["op"] ?? ""):\(payload["type"] ?? ""):\(payload["action"] ?? "")"}
    var spells=[Obj](),technologies=[Int:Obj]()
    let names: [String:String]
    init() {
        names=((try? loadObject(runtime.appendingPathComponent("unit-names.json")))?["types"] as? [String:String]) ?? [:]
    }
    func accept(_ response:Obj) {
        for row in response["types"] as? [Obj] ?? [] {costs[Int(number(row,"id"))]=row}
        for row in response["technologies"] as? [Obj] ?? [] {technologies[Int(number(row,"id"))]=row}
        spells=response["spells"] as? [Obj] ?? []
    }
    func choices(_ unit:Obj)->[CommandChoice] {
        let kind=Int(number(unit,"type")),building=unit["building"] as? Bool == true
        if kind>=176 || [4,6,18,24,26,31,33,73,85,89,90,93,94,95,96].contains(kind) {return []}
        var result=[CommandChoice]()
        if !building && ![35,36,59,97].contains(kind) {
            result += [CommandChoice("Move",["op":"move"],.ground),CommandChoice("Attack",["op":"attack_move"],.ground),
                       CommandChoice("Stop",["op":"stop"]),CommandChoice("Hold",["op":"hold"]),
                       CommandChoice("Patrol",["op":"patrol"],.ground)]
        } else if [106,111,113,114,130,131,132,133,154,155,160,167].contains(kind) {result.append(CommandChoice("Rally",["op":"rally"],.rally))}
        if unit["worker"] as? Bool == true {result.append(CommandChoice("Gather",["op":"gather"],.unit))}
        if unit["worker"] as? Bool == true && number(unit,"carrying") != 0 {result.append(CommandChoice("Return cargo",["op":"return_cargo"]))}
        if [131,132,133].contains(kind) {result.append(CommandChoice("Select larvae",["op":"select_larvae"]))}
        if [106,111,113,114,116,122].contains(kind) {
            result.append(CommandChoice(unit["airborne"] as? Bool == true ? "Land":"Lift off",["op":unit["airborne"] as? Bool == true ? "land":"lift","type":kind],unit["airborne"] as? Bool == true ? .ground:.none))
        }
        if kind==7 {result.append(CommandChoice("Repair",["op":"repair"],.unit))}
        for spell in spells where (spell["casters"] as? [Int] ?? []).contains(kind) {
            guard let action=spell["action"] as? String,let title=spell["title"] as? String else {continue}
            let cost=number(spell,"energy"),label=title+(cost>0 ? " \(Int(cost)) energy":"")
            result.append(CommandChoice(label,["op":"spell","action":action],spell["target"] as? String == "unit" ? .unit:.ground))
        }
        let builds:[Int:[Int]]=[7:[106,109,110,111,112,113,114,116,122,123,124,125],
            41:[131,149,142,135,139,141,138,136,140,134,143],64:[154,156,157,160,164,166,163,169,170,167,172,171,165,155,159]]
        let trains:[Int:[Int]]=[106:[7],111:[0,32,1,34],113:[2,5,3],114:[8,12,11,9,58],
            154:[64],160:[65,66,67,61],155:[69,83,84],167:[70,71,72,60],130:[50]]
        let morphs:[Int:[Int]]=[35:[41,37,38,43,45,46,42,39,47],38:[103],43:[44,62],131:[132],132:[133],141:[137],143:[144,146]]
        let addons:[Int:[Int]]=[106:[107,108],113:[120],114:[115],116:[117,118]]
        for (operation,table,target) in [("build",builds,CommandTarget.ground),("train",trains,.none),("morph",morphs,.none),("addon",addons,.ground)] {
            for type in table[kind] ?? [] {
                let dat=costs[type] ?? [:]
                let cost=costs[type]==nil ? "": " \(Int(number(dat,"mineralCost")))/\(Int(number(dat,"gasCost")))"
                result.append(CommandChoice((names[String(type)] ?? "Unit \(type)")+cost,["op":operation,"type":type],target))
            }
        }
        let research:[Int:[(Int,String)]]=[112:[(0,"Stim"),(24,"Restoration"),(30,"Optical flare")],
            120:[(5,"Siege"),(3,"Spider mines")],115:[(9,"Cloaking field")],117:[(1,"Lockdown"),(10,"Personnel cloak")],
            116:[(7,"Irradiate"),(2,"EMP")],131:[(11,"Burrow")],132:[(11,"Burrow")],133:[(11,"Burrow")],
            135:[(32,"Lurker aspect")],136:[(16,"Consume"),(15,"Plague")],138:[(13,"Spawn broodling"),(17,"Ensnare")],
            118:[(8,"Yamato gun")],169:[(25,"Disruption web")],
            165:[(19,"Psionic storm"),(20,"Hallucination"),(27,"Mind control"),(31,"Maelstrom")],170:[(21,"Recall"),(22,"Stasis")]]
        let upgrades:[Int:[(Int,String)]]=[122:[(7,"Infantry weapons"),(0,"Infantry armor")],
            123:[(8,"Vehicle weapons"),(9,"Ship weapons"),(1,"Vehicle armor"),(2,"Ship armor")],
            112:[(16,"U-238 shells"),(51,"Caduceus reactor")],120:[(17,"Ion thrusters"),(54,"Charon boosters")],
            116:[(19,"Titan reactor")],117:[(20,"Ocular implants"),(21,"Moebius reactor")],115:[(22,"Apollo reactor")],118:[(23,"Colossus reactor")],
            139:[(10,"Melee attacks"),(11,"Missile attacks"),(3,"Carapace")],
            141:[(12,"Flyer attacks"),(4,"Flyer carapace")],137:[(12,"Flyer attacks"),(4,"Flyer carapace")],
            132:[(24,"Ventral sacs"),(25,"Antennae"),(26,"Pneumatized carapace")],133:[(24,"Ventral sacs"),(25,"Antennae"),(26,"Pneumatized carapace")],
            142:[(27,"Metabolic boost"),(28,"Adrenal glands")],135:[(29,"Muscular augments"),(30,"Grooved spines")],
            138:[(31,"Gamete meiosis")],136:[(32,"Metasynaptic node")],140:[(52,"Chitinous plating"),(53,"Anabolic synthesis")],
            166:[(13,"Ground weapons"),(5,"Ground armor"),(15,"Plasma shields")],164:[(14,"Air weapons"),(6,"Air armor"),(33,"Singularity charge")],
            163:[(34,"Leg enhancements")],171:[(35,"Scarab damage"),(36,"Reaver capacity"),(37,"Gravitic drive")],
            159:[(38,"Sensor array"),(39,"Gravitic boosters")],165:[(40,"Khaydarin amulet"),(49,"Argus talisman")],
            169:[(41,"Apial sensors"),(42,"Gravitic thrusters"),(43,"Carrier capacity"),(47,"Argus jewel")],170:[(44,"Khaydarin core")]]
        for (operation,table) in [("research",research),("upgrade",upgrades)] {
            for (type,title) in table[kind] ?? [] {result.append(CommandChoice(title,["op":operation,"type":type]))}
        }
        if [0,32].contains(kind) {result.append(CommandChoice("Stim",["op":"ability","action":"stim"]))}
        if kind==5 || kind==30 {result.append(CommandChoice(kind==5 ? "Siege":"Unsiege",["op":"ability","action":kind==5 ? "siege":"unsiege"]))}
        if [37,38,41,46,103].contains(kind) {
            let buried=unit["burrowed"] as? Bool == true
            result.append(CommandChoice(buried ? "Unburrow":"Burrow",["op":"ability","action":buried ? "unburrow":"burrow"]))
        }
        if [1,8].contains(kind) {
            let cloaked=unit["cloaked"] as? Bool == true
            result.append(CommandChoice(cloaked ? "Decloak":"Cloak",["op":"ability","action":cloaked ? "decloak":"cloak"]))
        }
        if [11,42,69,125].contains(kind) {result += [CommandChoice("Load",["op":"load"],.unit),CommandChoice("Unload all",["op":"unload"])]}
        if building && (unit["buildQueue"] as? [Int] ?? []).contains(where:{$0<228}) {result.append(CommandChoice("Cancel train",["op":"cancel","action":"train"]))}
        if building && number(unit,"order")==75 {result.append(CommandChoice("Cancel research",["op":"cancel","action":"research"]))}
        if building && number(unit,"order")==76 {result.append(CommandChoice("Cancel upgrade",["op":"cancel","action":"upgrade"]))}
        if unit["constructingAddon"] as? Bool == true {result.append(CommandChoice("Cancel addon",["op":"cancel","action":"addon"]))}
        if unit["completed"] as? Bool == false && ![36,59,97].contains(kind) {result.append(CommandChoice("Cancel construction",["op":"cancel","action":"construction"]))}
        if [36,59,97].contains(kind) {result.append(CommandChoice("Cancel morph",["op":"cancel","action":"morph"]))}
        let hotkeys=["move":"M","attack_move":"A","stop":"S","hold":"H","patrol":"P","gather":"G","repair":"R","rally":"R","return_cargo":"C","lift":"L","land":"L","select_larvae":"S"]
        let unitKeys:[Int:String]=[7:"S",64:"P",106:"C",109:"S",110:"R",111:"B",112:"A",113:"F",114:"S",116:"I",122:"E",123:"A",124:"T",125:"U",107:"C",108:"N",131:"H",149:"E",142:"S",135:"H",139:"E",141:"S",138:"Q",136:"D",140:"U",134:"N",143:"C",154:"N",156:"P",157:"A",160:"G",164:"Y",166:"F",163:"C",169:"F",170:"A",167:"S",172:"B",171:"B",165:"T",155:"R",159:"O",132:"L",133:"H"]
        let priced=result.map { entry in
            var choice=entry
            let op=choice.payload["op"] as? String ?? "",id=Int(number(choice.payload,"type"))
            choice.hotkey=hotkeys[op] ?? ""
            if ["build","train","morph","addon"].contains(op) {
                let dat=costs[id] ?? [:];choice.priced=costs[id] != nil
                choice.mineralCost=number(dat,"mineralCost");choice.gasCost=number(dat,"gasCost")
                choice.race=Int(number(dat,"race"))
                choice.supplyCost=["train","morph"].contains(op) ? max(0,number(dat,"supplyCost")-(op=="morph" ? number(costs[kind] ?? [:],"supplyCost"):0)):0
                if [37,47].contains(id) {choice.supplyCost *= 2}
                let trainedKeys:[Int:String]=[0:"M",1:"G",2:"V",3:"G",5:"T",8:"W",9:"V",11:"D",12:"B",32:"F",34:"C",37:"Z",38:"H",39:"U",41:"D",42:"O",43:"M",44:"G",45:"Q",46:"D",47:"S",50:"I",58:"V",60:"O",61:"K",62:"D",65:"Z",66:"D",67:"T",69:"S",70:"S",71:"A",72:"C",83:"V",84:"O",103:"L"]
                choice.hotkey=unitKeys[id] ?? trainedKeys[id] ?? ""
            }
            if op=="research" {
                let dat=technologies[id] ?? [:];choice.priced=technologies[id] != nil
                choice.mineralCost=number(dat,"mineralCost");choice.gasCost=number(dat,"gasCost")
                if id==11 {choice.hotkey="B"}
            }
            if op=="spell",let spell=spells.first(where:{$0["action"] as? String==choice.payload["action"] as? String}) {choice.energyCost=number(spell,"energy")}
            if op=="ability" {choice.hotkey=["stim":"T","burrow":"B","unburrow":"U"][choice.payload["action"] as? String ?? ""] ?? ""}
            if op=="cancel" {choice.hotkey="Esc"}
            return choice
        }
        var used=Set<String>()
        let liveRows=availability["unit"] as? String == unit["id"] as? String ? availability["choices"] as? [Obj] ?? []:[]
        return priced.enumerated().map { index,entry in
            if index%9==0 {used.removeAll()}
            var choice=entry
            let op=choice.payload["op"] as? String ?? ""
            if ["build","train","morph","addon","research","upgrade","spell","ability","load","unload"].contains(op) {
                if let row=liveRows.first(where:{choiceKey($0["payload"] as? Obj ?? [:])==choiceKey(choice.payload)}) {
                    choice.energyCost=number(row,"energyCost")
                    choice.requirementReason=row["requirementsMet"] as? Bool == true ? nil:"Native requirements not met"
                    if ["build","train","morph","addon","research","upgrade"].contains(op) {
                        choice.mineralCost=number(row,"mineralCost");choice.gasCost=number(row,"gasCost");choice.priced=true
                    }
                } else {choice.requirementReason="Checking native requirements"}
            }
            if choice.hotkey.isEmpty || used.contains(choice.hotkey) {choice.hotkey=String(index%9+1)}
            used.insert(choice.hotkey)
            return choice
        }
    }
}
