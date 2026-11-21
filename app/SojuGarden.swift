import AppKit
import SwiftUI

/// The Soju Garden at the top of the library: a year of play as rows of shot glasses (30 minutes
/// of play pours one shot), like a contribution graph, with the app's bottle cheering you on.
/// Reads play-log.jsonl, which only this Mac keeps (scripts/play-stats.py). Korean when the Mac's
/// first language is Korean, English otherwise.
struct SojuGardenCard: View {
    @EnvironmentObject var model: SojuModel
    @State private var garden = Garden(sessions: [])
    private let refresh = Timer.publish(every: 120, on: .main, in: .common).autoconnect()
    private let ko = Locale.preferredLanguages.first?.hasPrefix("ko") == true

    @Environment(\.colorScheme) private var scheme
    // The app's accent (Library.swift, Soju.swift).
    private var green: Color { scheme == .dark ? Color(red: 0.42, green: 0.83, blue: 0.67) : Color(red: 0.12, green: 0.34, blue: 0.28) }
    private let cell: CGFloat = 14, gap: CGFloat = 3

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 0) {
                Image(nsImage: NSApp.applicationIconImage).resizable().frame(width: 52, height: 52).accessibilityHidden(true)
                Bubble(text: bubble).padding(.leading, 16).padding(.top, 9)
                Spacer(minLength: 12)
                VStack(alignment: .trailing, spacing: 4) {
                    Text(ko ? "소주잔디" : "Soju Garden").font(.system(size: 16, weight: .semibold))
                    Text(ko ? "게임 30분 = 소주 한 잔" : "30 minutes of play = 1 shot").font(.system(size: 12)).foregroundStyle(.secondary)
                    rankPill
                }
            }
            HStack(alignment: .firstTextBaseline, spacing: 22) {
                stat(garden.shotsThisYear, ko ? "잔 올해" : garden.shotsThisYear == 1 ? "shot this year" : "shots this year")
                stat(garden.streak, ko ? "일 연속" : "-day streak", tight: !ko)
                stat(garden.bestStreak, ko ? "일 최장 연속" : garden.bestStreak == 1 ? "day best streak" : "days best streak")
                stat(garden.shotsToday, ko ? "잔 오늘" : garden.shotsToday == 1 ? "shot today" : "shots today")
                if let top = garden.topGame {
                    HStack(spacing: 4) {
                        Text((ko ? "제일 많이 마신 게임" : "Most poured") + " ·").foregroundStyle(.secondary)
                        Text(top).fontWeight(.semibold).foregroundStyle(green).lineLimit(1)
                    }.font(.system(size: 13))
                }
            }
            // Left-aligned when the year fits; on a narrow window it scrolls, showing the latest weeks.
            ViewThatFits(in: .horizontal) {
                grid
                ScrollView(.horizontal, showsIndicators: false) { grid }.defaultScrollAnchor(.trailing)
            }
            HStack(alignment: .center, spacing: 8) {
                ForEach(shownBadges, id: \.0.id) { badge, left in chip(badge, left: left) }
                Spacer(minLength: 8)
                Text(ko ? "안 마심" : "None").font(.system(size: 11)).foregroundStyle(.secondary)
                ForEach(0..<5) { ShotGlass(level: $0, accent: green).frame(width: cell, height: cell) }
                Text(ko ? "원샷" : "Bottoms up").font(.system(size: 11)).foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .background(green.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.07), lineWidth: 1))
        .onAppear(perform: reload)
        .onChange(of: model.launchingGames) { _ in reload() }
        .onReceive(refresh) { _ in reload() }
    }

    private func reload() {
        let url = model.base.appendingPathComponent("play-log.jsonl")
        let text = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
        garden = Garden(sessions: Garden.parse(text))
    }

    private var bubble: String {
        let g = garden
        if !g.any { return ko ? "게임을 하면 첫 잔이 채워져요." : "Play a game to pour your first shot." }
        if g.playedToday && g.streak >= 2 { return ko ? "\(g.streak)일 연속 짠!" : "\(g.streak) days in a row. Cheers!" }
        if g.playedToday { return ko ? "오늘 \(g.shotsToday)잔 짠!" : "\(g.shotsToday) \(g.shotsToday == 1 ? "shot" : "shots") today. Cheers!" }
        if g.streak >= 2 { return ko ? "\(g.streak)일 연속 짠! 오늘도 한 잔 할까요?" : "\(g.streak) days in a row. One more shot today?" }
        if g.streak == 1 { return ko ? "어제 한 잔 했죠? 오늘도 한 잔 할까요?" : "One shot yesterday. Another today?" }
        return ko ? "오랜만이에요. 오늘 한 잔 할까요?" : "Been a while. One shot today?"
    }

    /// Medal colours from iron to grandmaster.
    private static let medal: [Color] = [Color(red: 0.45, green: 0.45, blue: 0.47), Color(red: 0.72, green: 0.45, blue: 0.24),
        Color(red: 0.6, green: 0.63, blue: 0.68), Color(red: 0.85, green: 0.65, blue: 0.13), Color(red: 0.2, green: 0.62, blue: 0.6),
        Color(red: 0.27, green: 0.55, blue: 0.9), Color(red: 0.6, green: 0.35, blue: 0.85), Color(red: 0.86, green: 0.25, blue: 0.3)]

    private var rankPill: some View {
        let r = garden.rank
        let next = garden.nextRank.map { ko ? " · \($0.0.ko)까지 \($0.1)잔" : " · \($0.1) to \($0.0.en)" } ?? ""
        return HStack(spacing: 5) {
            Image(systemName: r.tier >= 6 ? "crown.fill" : "shield.fill").font(.system(size: 11)).foregroundStyle(Self.medal[r.tier])
            Text(ko ? r.ko : r.en).font(.system(size: 12, weight: .semibold))
            Text(next).font(.system(size: 11)).foregroundStyle(.secondary)
        }
        .padding(.horizontal, 8).padding(.vertical, 3)
        .background(Color.primary.opacity(0.05), in: Capsule())
        .help(ko ? "계급: 마신 잔 수로 올라가요 (아이언부터 그랜드마스터까지)" : "Rank: goes up with the shots you pour, from Iron to Grandmaster")
    }

    private func stat(_ n: Int, _ label: String, tight: Bool = false) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: tight ? 0 : 3) {
            Text(String(n)).font(.system(size: 18, weight: .semibold, design: .rounded)).foregroundStyle(green)
            Text(label).font(.system(size: 13)).foregroundStyle(.secondary)
        }
    }

    /// Earned badges, then the next play-time badge greyed out with the shots it still needs.
    private var shownBadges: [(Garden.Badge, Int?)] {
        var out: [(Garden.Badge, Int?)] = garden.badges.filter { $0.earned }.map { ($0, nil) }
        if let (next, left) = garden.nextBadge { out.append((next, left)) }
        return out
    }

    private func chip(_ b: Garden.Badge, left: Int?) -> some View {
        let name = ko ? b.ko : b.en
        let more = left.map { ko ? " · \($0)잔 남음" : " · \($0) more \($0 == 1 ? "shot" : "shots")" } ?? ""
        return HStack(spacing: 4) {
            ShotGlass(level: b.id == "first" ? 2 : b.shots != nil ? 3 : 4, accent: green).frame(width: 12, height: 12)
            Text(name + more).font(.system(size: 12, weight: .medium)).lineLimit(1)
        }
        .padding(.horizontal, 9).padding(.vertical, 4)
        .foregroundStyle(left == nil ? green : Color.secondary)
        .background(left == nil ? green.opacity(0.13) : Color.primary.opacity(0.05), in: Capsule())
        .help(left == nil ? name : (ko ? "다음 배지: " : "Next badge: ") + name + more)
    }

    private var grid: some View {
        let cal = Calendar.current
        let f = DateFormatter()
        f.locale = Locale(identifier: ko ? "ko_KR" : "en_US")
        f.setLocalizedDateFormatFromTemplate("MMM")
        let firstDays = garden.weeks.map { $0.first ?? nil }
        return HStack(alignment: .top, spacing: 6) {
            VStack(alignment: .trailing, spacing: gap) {
                Color.clear.frame(width: 1, height: 12)
                ForEach(0..<7) { r in
                    Text(r % 2 == 1 ? weekday(r, cal) : "").font(.system(size: 10)).foregroundStyle(.secondary).frame(height: cell)
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: gap) {
                    ForEach(Array(firstDays.enumerated()), id: \.offset) { i, day in
                        let label: String = {
                            guard let d = day, i < firstDays.count - 2 else { return "" }
                            let prev = i > 0 ? firstDays[i - 1] : nil
                            return prev.map { cal.component(.month, from: $0.date) != cal.component(.month, from: d.date) } ?? true
                                ? f.string(from: d.date) : ""
                        }()
                        Text(label).font(.system(size: 10)).foregroundStyle(.secondary).fixedSize().frame(width: cell, height: 12, alignment: .leading)
                    }
                }
                HStack(alignment: .top, spacing: gap) {
                    ForEach(Array(garden.weeks.enumerated()), id: \.offset) { _, week in
                        VStack(spacing: gap) {
                            ForEach(0..<7) { r in
                                if let day = week[r] {
                                    ShotGlass(level: day.level, accent: green).frame(width: cell, height: cell).help(tip(day))
                                } else {
                                    Color.clear.frame(width: cell, height: cell)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    private func weekday(_ row: Int, _ cal: Calendar) -> String {
        let symbols = ko ? ["일", "월", "화", "수", "목", "금", "토"] : ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
        return symbols[(cal.firstWeekday - 1 + row) % 7]
    }

    private func tip(_ d: GardenDay) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: ko ? "ko_KR" : "en_US")
        f.setLocalizedDateFormatFromTemplate(ko ? "MMMdE" : "EMMMd")
        let date = f.string(from: d.date)
        if d.minutes == 0 { return date + " · " + (ko ? "안 마심" : "no shots") }
        let h = d.minutes / 60, m = d.minutes % 60
        let time = ko ? (h > 0 ? "\(h)시간 \(m)분" : "\(m)분") : (h > 0 ? "\(h)h \(m)m" : "\(m)m")
        let shots = ko ? "\(d.shots)잔" : "\(d.shots) \(d.shots == 1 ? "shot" : "shots")"
        return ([date + " · " + shots + ", " + time] + (d.titles.isEmpty ? [] : [d.titles.joined(separator: ", ")])).joined(separator: "\n")
    }
}

/// The app icon's speech: a white box with its tail pointing left at the bottle, set apart from it.
private struct Bubble: View {
    var text: String
    var body: some View {
        Text(text).font(.system(size: 14, weight: .medium))
            .padding(.horizontal, 11).padding(.vertical, 7)
            .background(Color.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 9))
            .background(alignment: .leading) {
                Tail().fill(Color.primary.opacity(0.06)).frame(width: 7, height: 10).offset(x: -7)
            }
    }
}

private struct Tail: Shape {
    func path(in r: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: r.maxX, y: r.minY)); p.addLine(to: CGPoint(x: r.minX, y: r.midY))
        p.addLine(to: CGPoint(x: r.maxX, y: r.maxY)); p.closeSubpath()
        return p
    }
}

/// A shot glass like the one in the app icon: 0 an empty dashed glass, then fuller in the accent
/// colour; from 3 it smiles, at 4 it sparkles.
struct ShotGlass: View {
    var level: Int
    var accent: Color
    var body: some View {
        Canvas { ctx, size in
            let s = size.width / 16
            func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x * s, y: y * s) }
            func poly(_ pts: [(CGFloat, CGFloat)]) -> Path {
                var path = Path(); path.addLines(pts.map { p($0.0, $0.1) }); path.closeSubpath(); return path
            }
            let l = max(0, min(4, level))
            if l > 0 {
                let y = 14 - [0, 4.5, 7.5, 10.5, 12.6][l]
                ctx.fill(poly([(3.6, y), (12.4, y), (11.3, 14), (4.7, 14)]), with: .color(accent.opacity([0, 0.35, 0.55, 0.8, 1][l])))
            }
            let glass = poly([(2.6, 2.2), (13.4, 2.2), (11.6, 14.4), (4.4, 14.4)])
            if l == 0 {
                ctx.stroke(glass, with: .color(Color.primary.opacity(0.2)),
                           style: StrokeStyle(lineWidth: 1.1 * s, lineJoin: .round, dash: [1.6 * s, 1.4 * s]))
                return
            }
            ctx.stroke(glass, with: .color(Color.primary.opacity(0.75)), style: StrokeStyle(lineWidth: 1.2 * s, lineJoin: .round))
            let face = Color(nsColor: .windowBackgroundColor)   // reads on the accent in light and dark mode
            if l >= 3 {
                for x in [6.3, 9.7] { ctx.fill(Path(ellipseIn: CGRect(x: (x - 0.75) * s, y: 8.85 * s, width: 1.5 * s, height: 1.5 * s)), with: .color(face)) }
                var smile = Path(); smile.move(to: p(6.8, 11.3)); smile.addQuadCurve(to: p(9.2, 11.3), control: p(8, 12.4))
                ctx.stroke(smile, with: .color(face), style: StrokeStyle(lineWidth: 0.8 * s, lineCap: .round))
                for x in [5.2, 10.8] {
                    ctx.fill(Path(ellipseIn: CGRect(x: (x - 0.8) * s, y: 11.35 * s, width: 1.6 * s, height: 0.9 * s)),
                             with: .color(Color(red: 0.96, green: 0.65, blue: 0.65).opacity(0.8)))
                }
            }
            if l == 4 {
                ctx.fill(poly([(14.2, 0.6), (14.7, 1.7), (15.8, 2.2), (14.7, 2.7), (14.2, 3.8), (13.7, 2.7), (12.6, 2.2), (13.7, 1.7)]),
                         with: .color(Color(red: 0.91, green: 0.64, blue: 0.24)))
            }
        }
        .accessibilityHidden(true)
    }
}
