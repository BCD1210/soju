import Foundation

/// DownloadMeter reads curl's meter as the app's logs hold it (lines copied from curl 8 on macOS).
@main struct DownloadMeterTests {
    static func main() {
        let log = """
          downloading engine-v1.11
          % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current
                                         Dload  Upload   Total   Spent    Left  Speed
         23 1474k   23  348k    0     0   298k      0  0:00:04  0:00:01  0:00:03  298k
         46 1474k   46  684k    0     0   310k      0  0:00:04  0:00:02  0:00:02  323k
        """
        let m = DownloadMeter.last(in: log)!
        assert(m.fraction == 0.46 && !m.done)
        assert(m.detail == "310 KB/s · 0:02 left", m.detail)

        let unknown = DownloadMeter.last(in: " 84 1474k   84 1244k    0     0   299k      0  0:00:04  0:00:04 --:--:--  299k")!
        assert(unknown.detail == "299 KB/s", unknown.detail)

        let hours = DownloadMeter.last(in: "  3  9.8G    3  301M    0     0  1.2M      0  2:19:01  0:04:10  2:14:51  1.1M")!
        assert(hours.fraction == 0.03 && hours.detail == "1.2 MB/s · 2:14:51 left", hours.detail)

        assert(DownloadMeter.last(in: "100  512M  100  512M    0     0  20.1M      0  0:00:25  0:00:25 --:--:-- 21.0M")!.done)
        let bare = DownloadMeter.last(in: " 19 1474k   19  287k    0     0  53562      0  0:00:28  0:00:05  0:00:23 51200")!
        assert(bare.detail == "52 KB/s · 0:23 left", bare.detail)

        assert(DownloadMeter.last(in: "  engine updated to engine-v1.11: wine-11.0") == nil)
        assert(DownloadMeter.last(in: "") == nil)
        print("download meter: ok")
    }
}
