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

func originalCommandAudit() throws {
    let bridge=Bridge(),card=CommandCard()
    card.accept(try bridge.request(["op":"unit_types"]))
    let snapshot=try bridge.request(["op":"state"])
    let units=(snapshot["units"] as? [Obj] ?? []).filter{number($0,"owner")==number(snapshot,"player")}
    var cases=[Obj]()
    func record(_ selection:[Obj],menu:Int,response:Obj) {
        let ids=selection.compactMap{$0["id"] as? String}
        card.nativeMenu=menu;card.acceptOriginal(response,selection:ids,menu:menu)
        let choices=card.choices(selection[0])
        cases.append(["units":selection,"menu":menu,"native":response,"renderedCard":choices.map {choice -> Obj in
            ["position":choice.originalPosition ?? 0,"hotkey":choice.hotkey,"title":choice.title,
             "enabled":choice.unavailable(snapshot,selection[0])==nil,"payload":choice.payload,
             "recordIndex":choice.originalRecord?["recordIndex"] ?? -1,
             "icon":choice.originalRecord?["icon"] ?? -1,
             "enabledString":choice.originalRecord?["enabledString"] ?? -1,
             "disabledString":choice.originalRecord?["disabledString"] ?? -1]
        }])
    }
    func sample(_ selection:[Obj],menu:Int=228) throws {
        let ids=selection.compactMap{$0["id"] as? String}
        let response=try bridge.request(["op":"evaluated_buttons","units":ids,"menu":menu])
        record(selection,menu:menu,response:response)
        if menu==228 {
            for row in response["buttons"] as? [Obj] ?? [] where row["family"] as? String == "submenu" && number(row,"conditionState") != 0 {
                let child=Int(number(row,"actionParameter"))
                if child != 228 {try sample(selection,menu:child)}
            }
        }
    }
    if let input=argument("--original-card-input") {
        let saved=try loadObject(URL(fileURLWithPath:input))
        for entry in saved["cases"] as? [Obj] ?? [] {
            guard let selection=entry["units"] as? [Obj],!selection.isEmpty,let response=entry["native"] as? Obj else {throw SceneError.invalid("Invalid original card sample")}
            record(selection,menu:Int(number(entry,"menu")),response:response)
        }
    } else {
        for unit in units {try sample([unit])}
        let mobile=units.filter{$0["building"] as? Bool != true && number($0,"type")<176}
        for i in mobile.indices {
            for j in mobile.indices where j>i {try sample([mobile[i],mobile[j]])}
        }
    }
    let report:Obj=["snapshot":snapshot,"cases":cases,"source":"live-native-conditions-and-command-card"]
    guard let path=argument("--original-card-audit") else {throw SceneError.invalid("Missing card audit output")}
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:URL(fileURLWithPath:path))
    print("Original card samples: \(cases.count)")
}
