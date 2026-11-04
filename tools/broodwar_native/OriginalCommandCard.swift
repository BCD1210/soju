import Foundation

extension CommandCard {
    func acceptOriginal(_ response:Obj,selection:[String],menu:Int) {
        nativeMode=true
        guard nativeMenu==menu else {return}
        nativeSelection=selection;nativeResponse=response
        if let root=response["rootSet"] as? Int {nativeRoot=root}
    }
    func originalChoices(_ unit:Obj)->[CommandChoice] {
        guard nativeResponse["evaluated"] as? Bool == true,
              nativeSelection.first == unit["id"] as? String else {return []}
        var result=[CommandChoice]()
        for row in nativeResponse["buttons"] as? [Obj] ?? [] where Int(number(row,"conditionState")) != 0 {
            let position=Int(number(row,"position")),parameter=Int(number(row,"actionParameter"))
            guard (1...9).contains(position) else {continue}
            let family=row["family"] as? String ?? "unsupported"
            var payload:Obj=["op":family],target=CommandTarget.none
            switch family {
            case "move","attack_move","patrol":target = .ground
            case "gather","repair","load":target = .unit
            case "rally":target = .rally
            case "build","land","nydus_exit":payload["type"]=parameter;target = .ground
            case "train","morph","addon","research","upgrade","train_ammunition":payload["type"]=parameter
            case "submenu":payload["type"]=parameter
            case "stim","cloak","decloak","siege","unsiege","burrow","unburrow":payload=["op":"ability","action":family]
            case "nuclear_strike","heal","recharge_shields":
                payload=["op":"spell","action":family];target = family=="heal" ? .unit:.ground
            case "tech":
                if let spell=spells.first(where:{Int(number($0,"tech"))==parameter && $0["tech"] is NSNumber}) {
                    payload=["op":"spell","action":spell["action"] ?? ""]
                    target=spell["target"] as? String == "unit" ? .unit:.ground
                } else {payload=["op":"unsupported_tech","type":parameter]}
            case "cancel_morph","cancel_train","cancel_addon","cancel_research","cancel_upgrade","cancel_construction":
                payload=["op":"cancel","action":String(family.dropFirst(7))]
            default:break
            }
            let stringID=Int(number(row,"enabledString"))
            let text=originalStrings[String(stringID)] as? Obj ?? [:]
            var choice=CommandChoice(text["label"] as? String ?? "Command \(stringID)",payload,target)
            choice.hotkey=text["hotkey"] as? String ?? ""
            choice.originalPosition=position;choice.originalState=Int(number(row,"conditionState"));choice.originalRecord=row
            result.append(choice)
        }
        return result
    }
}
