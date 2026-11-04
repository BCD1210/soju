"""Transport regression tests; these do not claim to test gameplay."""
import pathlib, struct, subprocess, tempfile, unittest
from wire import CHUNK, HEADER, receive

def packets(data, response=7):
    count=(len(data)+CHUNK-1)//CHUNK
    return [HEADER.pack(b"SBW1",response,i,count,len(data))+data[i*CHUNK:(i+1)*CHUNK] for i in range(count)]

class Socket:
    def __init__(self, chunks): self.chunks=iter(chunks)
    def recv(self, _):
        try: return next(self.chunks)
        except StopIteration: raise TimeoutError("missing chunk")

class WireTests(unittest.TestCase):
    def test_plain(self):
        self.assertEqual(receive(Socket([b'{"ok":true}'])),b'{"ok":true}')
    def test_large_and_reordered(self):
        for size in [10000,100000,2*1024*1024]:
            data=bytes(i%251 for i in range(size))
            self.assertEqual(receive(Socket(list(reversed(packets(data))))),data)
    def test_missing(self):
        with self.assertRaises(TimeoutError): receive(Socket(packets(b"x"*100000)[:-1]))
    def test_duplicate(self):
        p=packets(b"x"*10000)
        with self.assertRaises(ValueError): receive(Socket([p[0],p[0]]))
    def test_mixed(self):
        a=packets(b"x"*10000,7);b=packets(b"x"*10000,8)
        with self.assertRaises(ValueError): receive(Socket([a[0],b[1]]))
    def test_bounds(self):
        for p in [b"SBW1",HEADER.pack(b"SBW1",1,0,257,2*1024*1024+1),HEADER.pack(b"SBW1",1,2,2,10000),HEADER.pack(b"SBW1",1,0,0,0)]:
            with self.assertRaises(ValueError): receive(Socket([p]))
    def test_swift_decoder(self):
        source=pathlib.Path(__file__).with_name("SceneSupport.swift").read_text()
        decoder=source[source.index("struct SnapshotWire {"):source.index("final class Bridge {")]
        u32=source[source.index("func u32("):source.index("// A snapshot is applied")]
        main=r'''
func require(_ ok:Bool) { if !ok { fatalError("wire assertion failed") } }
func packet(_ data:[UInt8],_ index:Int,_ response:UInt32=7)->[UInt8] {
    let count=(data.count+8175)/8176
    var out:[UInt8]=[83,66,87,49]
    for v in [response] { for shift in stride(from:0,to:32,by:8) { out.append(UInt8(truncatingIfNeeded:v>>shift)) } }
    for v in [index,count] { out.append(UInt8(truncatingIfNeeded:v));out.append(UInt8(truncatingIfNeeded:v>>8)) }
    for shift in stride(from:0,to:32,by:8) { out.append(UInt8(truncatingIfNeeded:data.count>>shift)) }
    out += data[index*8176..<min(data.count,(index+1)*8176)]
    return out
}
for size in [10000,100000,2*1024*1024] {
    let input=(0..<size).map{UInt8($0%251)};var wire=SnapshotWire();var result:Data?
    for i in (0..<((size+8175)/8176)).reversed() { result=try wire.accept(packet(input,i)) }
    require(result==Data(input))
}
let input=[UInt8](repeating:1,count:10000)
var partial=SnapshotWire();require(try partial.accept(packet(input,0))==nil)
do { _=try partial.accept(packet(input,0));fatalError("duplicate accepted") } catch {}
var mixed=SnapshotWire();_=try mixed.accept(packet(input,0))
do { _=try mixed.accept(packet(input,1,9));fatalError("mixed response accepted") } catch {}
var invalid=SnapshotWire()
do { _=try invalid.accept([83,66,87,49]);fatalError("short header accepted") } catch {}
print("Swift snapshot reassembly: PASS")
'''
        with tempfile.TemporaryDirectory() as tmp:
            script=pathlib.Path(tmp)/"wire.swift"
            script.write_text('import Foundation\nenum SceneError:Error {case invalid(String)}\n'+u32+decoder+main)
            subprocess.run(["swift",str(script)],check=True,timeout=45)

if __name__=="__main__": unittest.main()