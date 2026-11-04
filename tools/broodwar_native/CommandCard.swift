import Foundation

enum CommandTarget { case none, ground, unit, rally }
struct CommandChoice {
    let title:String,payload:Obj,target:CommandTarget
    init(_ title:String,_ payload:Obj,_ target:CommandTarget = .none) {
        self.title=title;self.payload=payload;self.target=target
    }
}
// Producer relationships are commander rules; unit costs come from native units.dat.
// The original command handler remains authoritative for prerequisites and availability.
final class CommandCard {
    var costs=[Int:Obj]()
    let names: [String:String]
    init() {
        names=((try? loadObject(runtime.appendingPathComponent("unit-names.json")))?["types"] as? [String:String]) ?? [:]
    }
    func accept(_ response:Obj) {
        for row in response["types"] as? [Obj] ?? [] {costs[Int(number(row,"id"))]=row}
    }
    func choices(_ unit:Obj)->[CommandChoice] {
        let kind=Int(number(unit,"type")),building=unit["building"] as? Bool == true
        var result=[CommandChoice]()
        if !building {
            result += [CommandChoice("Move",["op":"move"],.ground),CommandChoice("Attack",["op":"attack_move"],.ground),
                       CommandChoice("Stop",["op":"stop"]),CommandChoice("Hold",["op":"hold"])]
        } else {result.append(CommandChoice("Rally",["op":"rally"],.rally))}
        if unit["worker"] as? Bool == true {result.append(CommandChoice("Gather",["op":"gather"],.unit))}
        if kind==7 {result.append(CommandChoice("Repair",["op":"repair"],.unit))}
        let builds:[Int:[Int]]=[7:[106,109,110,111,112,113,114,116,122,123,124,125],
            41:[131,149,142,135,139,141,138,136,140,134,143],64:[154,156,157,160,164,166,163,169,170,167,172,171,165,155]]
        let trains:[Int:[Int]]=[106:[7],111:[0,32,1,34],113:[2,5,3],114:[8,12,11,9,58],
            154:[64],160:[65,66,67,61],155:[69,83,84],167:[70,71,72,60]]
        let morphs:[Int:[Int]]=[35:[41,37,38,43,45,46,42,39,50],38:[103],43:[44,62],131:[132],132:[133],141:[137],143:[144,146]]
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
            135:[(32,"Lurker aspect")],136:[(15,"Consume"),(16,"Plague")],138:[(13,"Spawn broodling"),(17,"Ensnare")],
            165:[(19,"Psionic storm"),(20,"Hallucination")],170:[(21,"Recall"),(22,"Stasis")]]
        let upgrades:[Int:[(Int,String)]]=[122:[(7,"Infantry weapons"),(0,"Infantry armor")],
            123:[(8,"Vehicle weapons"),(9,"Ship weapons"),(1,"Vehicle armor"),(2,"Ship armor")],
            112:[(16,"U-238 shells")],120:[(17,"Ion thrusters")],139:[(10,"Melee attacks"),(11,"Missile attacks"),(3,"Carapace")],
            166:[(13,"Ground weapons"),(5,"Ground armor"),(15,"Plasma shields")],164:[(14,"Air weapons"),(6,"Air armor")]]
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
        if building {result += [CommandChoice("Cancel train",["op":"cancel","action":"train"]),
            CommandChoice("Cancel research",["op":"cancel","action":"research"]),CommandChoice("Cancel upgrade",["op":"cancel","action":"upgrade"])]}
        if unit["completed"] as? Bool == false {result.append(CommandChoice("Cancel construction",["op":"cancel","action":"construction"]))}
        if [36,59,97].contains(kind) {result.append(CommandChoice("Cancel morph",["op":"cancel","action":"morph"]))}
        return result
    }
}
