import Foundation

// Eviction drops cache ownership only; a draw's referenced resources stay alive.
// Byte limits cover cached resources, not resources pinned by active scene nodes.
final class ResidentCache<Value> {
    private struct Entry {var value:Value;var bytes:Int;var age:UInt64}
    private var entries=[String:Entry](),clock:UInt64=0
    let limit:Int
    private(set) var bytes=0
    private(set) var evictions=0
    init(limit:Int) {self.limit=max(0,limit)}
    func get(_ key:String)->Value? {
        guard var entry=entries[key] else {return nil}
        clock &+= 1;entry.age=clock;entries[key]=entry;return entry.value
    }
    func insert(_ value:Value,key:String,bytes cost:Int) {
        if let old=entries.removeValue(forKey:key) {bytes-=old.bytes}
        let cost=max(0,cost)
        guard cost<=limit else {return}
        while bytes+cost>limit,let oldest=entries.min(by:{$0.value.age<$1.value.age}) {
            bytes-=oldest.value.bytes;entries.removeValue(forKey:oldest.key);evictions+=1
        }
        clock &+= 1;entries[key]=Entry(value:value,bytes:cost,age:clock);bytes+=cost
    }
}
