import Foundation

// Audit real choices for every DAT row and alternate native state. This proves
// reachability and dispatch uniqueness, not equivalence to an unexported buttonset.
func commandAudit()throws {
    let metadata=try Bridge().request(["op":"unit_types"]),card=CommandCard()
    card.accept(metadata)
    guard let types=metadata["types"] as? [Obj],types.count==228 else {throw SceneError.invalid("Expected all 228 units.dat rows")}
    var rows=[Obj](),failures=[String]()
    for definition in types {
        let kind=Int(number(definition,"id"))
        for alternate in [false,true] {
            var unit=definition;unit["type"]=kind;unit["completed"]=true
            unit["airborne"]=alternate;unit["burrowed"]=alternate;unit["cloaked"]=alternate
            unit["carrying"]=alternate ? 1:0
            let choices=card.choices(unit)
            for start in stride(from:0,to:choices.count,by:9) {
                let page=choices[start..<min(start+9,choices.count)]
                let keys=page.map{$0.hotkey.lowercased()}
                if keys.contains("") || Set(keys).count != keys.count {failures.append("Type \(kind), page \(start/9): ambiguous hotkeys")}
            }
            if kind>=176 && !choices.isEmpty {failures.append("Neutral type \(kind) exposes commands")}
            if [35,36,59,97].contains(kind) && choices.contains(where:{$0.payload["op"] as? String == "move"}) {failures.append("Immobile type \(kind) exposes move")}
            rows.append(["type":kind,"alternateState":alternate,"commands":choices.map{["title":$0.title,"hotkey":$0.hotkey,"payload":$0.payload,"priced":$0.priced] as Obj}])
        }
    }
    let report:Obj=["typeCount":types.count,"rows":rows,"passed":failures.isEmpty,"failures":failures,
        "nativeButtonsetParity":false,"limitations":["Original executable buttonsets and prerequisite predicates are not exported.","Hero-specific abilities, carrier/reaver ammunition and archon merges remain incomplete.","Numeric keys resolve missing or colliding shortcuts on a commander page; these are not native hotkeys."]]
    guard let target=argument("--command-audit") else {throw SceneError.invalid("Missing audit output")}
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:URL(fileURLWithPath:target))
    if !failures.isEmpty {throw SceneError.invalid("Command audit: \(failures)")}
    print("Command audit passed: \(types.count) types, \(rows.count) states")
}
