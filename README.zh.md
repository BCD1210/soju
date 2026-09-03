# Soju（简体中文）

> *Wine → Whisky → Kegworks…… 这一轮来自韩国：**Soju** 🍶*

**登录器真的能登录。** 在 Apple Silicon Mac 上运行战网（Battle.net）、Steam、Epic Games Launcher 和 GOG GALAXY。没有黑色登录窗口，没有永远转圈的 "Signing in…"，也不需要 CrossOver 许可证。完全免费的开源 Wine 栈。

如果你是从 Whisky/Kegworks 相关帖子（"战网登录时崩溃"、"Steam 打不开"、"登录器窗口一片空白"）找到这里的：这些正是本仓库记录并解决的问题（CEF 渲染进程在 `PAGE_WRITECOPY` 上触发 `int3`、CEF GPU 进程初始化即死、Steam 的 webhelper）。详见 [docs/DIAGNOSIS.md](docs/DIAGNOSIS.md)。

引擎由 CodeWeavers 公开的 GPL 源码（Wine 11.0，CrossOver 26.3 源码包）用本仓库的脚本编译组装而成。不需要任何付费软件。

> 状态（2026-08）：**全链路可用**：战网登录、Agent、D2R 进入游戏并正常渲染（D3DMetal）；Epic Games Launcher 登录和游戏安装（菜单栏托盘图标）；GOG GALAXY 登录；Steam 客户端登录 + D3D11 游戏。在 M4 Pro / macOS 26.5 上验证。

*[English README](README.md) · [한국어 README](README.ko.md)*

## 能跑什么

<!-- 截图：把 PNG（d2r-ingame.png、battlenet-login.png、hogwarts-ingame.png）放进 docs/images/ 后取消注释。
<p align="center">
  <img src="docs/images/d2r-ingame.png" width="49%" alt="M4 Pro 上运行的 Diablo II: Resurrected">
  <img src="docs/images/battlenet-login.png" width="49%" alt="Apple Silicon 上已登录的战网">
</p>
-->

均在 M4 Pro / macOS 26.5 上验证。同一个引擎，每个登录器一个独立 bottle。

| 登录器 | 登录 | 安装游戏 | 说明 |
| --- | :-: | :-: | --- |
| 战网（Battle.net） | ✅ | ✅ | Agent 正常；`BLZBNTBNA00000005` 通过签名 exe 播种解决 |
| Epic Games Launcher | ✅ | ✅ | 关闭窗口后驻留菜单栏托盘 |
| GOG GALAXY | ✅ | ✅ | 登录和游戏库可用；游戏尚未广泛测试 |
| Steam（Windows 客户端） | ✅ | ✅ | 独立的 `wine-stable` + DXMT bottle |

| 游戏 | 平台 | 图形 | 状态 | 说明 |
| --- | --- | --- | :-: | --- |
| Diablo II: Resurrected | 战网 | D3DMetal (D3D11) | ✅ 进入游戏，可联网 | **需要 macOS 26.4 或更高**（见前提条件）；`play.sh` 设置 `ROSETTA_ADVERTISE_AVX=1`；[指南](https://bcd1210.github.io/soju/guides/diablo-2-resurrected-apple-silicon.html) |
| Hogwarts Legacy | Epic | D3DMetal (D3D12) | ✅ 进入游戏 | UE4 的"AMD 驱动"警告无害 |
| Unity D3D11 游戏 | Steam | DXMT | ✅ 进入游戏，窗口模式 | 需要 `soju steam-games`，见 [docs/STEAM-GAMES.md](docs/STEAM-GAMES.md) |
| Diablo II: Resurrected（Infernal Edition） | Steam | — | ⏳ 尚未验证 | Steam bottle 没有 GPTK，而 D2R 的加载器需要 `libd3dshared` |

跑不了的：任何带内核级反作弊（EAC、BattlEye、Vanguard）的游戏。跑通了别的游戏？欢迎发到 [Discussions](https://github.com/BCD1210/soju/discussions)，或提 PR 加一行。

## 为什么会有这个项目

社区 Wine 构建（Whisky、Kegworks 时代的引擎）已经无法运行新版战网和 D2R。商业方案可以用，但其底层 Wine 引擎是 GPL 的，所以我们自己把它编译出来，并记录了撞上的每一堵墙。其中三堵墙此前没有公开解法：

### 三把钥匙

1. **`ROSETTA_ADVERTISE_AVX=1`**：D2R 的加载器要求 AVX 指令集。没有这个环境变量，游戏会在任何图形初始化之前，卡在约 86 MB 内存、0% CPU 的状态永远不动。这就是非 CrossOver 的 Wine 构建上 "D2R 启动即卡死" 的真相。

2. **D3DMetal 载荷布局**：Apple Game Porting Toolkit 的载荷分三部分，三者齐全 D3DMetal 才会生效：`lib/external/`（libd3dshared + D3DMetal.framework）；Apple 的 PE 垫片 `d3d11.dll`、`d3d12.dll`、`dxgi.dll`（以及 `atidxx64`、`nvapi64`、`nvngx`），放在 `lib/wine/x86_64-windows/` 下替换 Wine 自带的同名 DLL；以及每个垫片对应一个 `lib/wine/x86_64-unix/<垫片>.so`，必须是指向 `lib/external/` 的**符号链接**。直接复制文件会破坏 `@loader_path` 解析，D3DMetal 会陷入断言循环；若保留 Wine 自带的 `d3d11.dll`，D3D 会走 wined3d，Epic Games Launcher 启动即崩溃。`soju gptk` 会安装全部三部分。

3. **战网 Agent 调用方签名修复**：Agent 校验连接客户端的签名时，用的是相对文件名，相对于它自己的工作目录（一个带版本号的子目录）解析。把签名过的 `Battle.net.exe` 复制一份到每个 `Battle.net.NNNNN` 子目录即可通过校验（修复错误 `BLZBNTBNA00000005`）。

另外一个陷阱：启动链里绝不能出现 Apple 平台二进制（`nohup`、`arch` 等）。macOS 在 exec 它们时会剥掉 `DYLD_*` 环境变量。

**指南（英文）：** [D2R on Apple Silicon](https://bcd1210.github.io/soju/guides/diablo-2-resurrected-apple-silicon.html) · [Windows Steam client](https://bcd1210.github.io/soju/guides/steam-windows-client-apple-silicon.html) · [Whisky alternatives](https://bcd1210.github.io/soju/guides/whisky-alternative.html)

## 安装（一行命令）

```bash
curl -fsSL soju.snack-wrap.com/install.sh | bash
```

或者用 Homebrew：

```bash
brew install BCD1210/soju/soju
soju install     # 然后：soju battlenet / soju d2r / soju steam / soju epic / soju gog
```

安装器会下载预编译引擎（约 350 MB），引导你下载 Apple 免费的 GPTK（唯一一个 Apple 禁止再分发的文件），然后询问你要装哪些登录器（任意组合：战网、Steam、Epic Games Launcher、GOG GALAXY），用各自的官方安装包装进各自独立的 bottle，并在 `~/Applications` 里为每个登录器生成可双击的 app（`Battle.net.app`、`Steam (Windows).app`、`Epic Games Launcher.app`、`GOG GALAXY.app`）。非交互模式：`SOJU_PLATFORMS=battlenet,epic,gog curl ... | bash`。登录，开玩。

## 日常维护

```bash
soju doctor      # 体检：检查整个栈，指出哪里不对（提 issue 时请附上输出）
soju update      # 更新脚本和预编译引擎（保留 GPTK 和所有 bottle）
soju uninstall   # 卸载：逐项确认后删除 app、bottle 和引擎
```

## 或者完全自己构建：不需要 CrossOver

除了本仓库脚本之外，你唯一要自己下载的是 Apple 免费的 Game Porting Toolkit（一个文件，一个免费 Apple ID 即可）。Apple 禁止再分发它。

```bash
# 1. 获取脚本
git clone https://github.com/BCD1210/soju.git && cd soju

# 2. 获取运行时组件（x86_64 动态库 + wine-mono，全部来自免费的 GPL 发布）
scripts/get-components.sh

# 3. 从 GPL 源码构建引擎（30-60 分钟）
scripts/build-engine.sh

# 4. 安装 Apple GPTK（从 https://developer.apple.com/games/game-porting-toolkit/
#    下载 "evaluation environment for Windows games" dmg 并挂载，然后：）
scripts/get-gptk.sh
#    （如果你恰好装了 CrossOver，脚本会自动从中提取）

# 5. 创建 bottle + 安装战网（Blizzard 官方安装包，全自动）
scripts/create-bottle.sh

# 6. 开玩
scripts/play.sh battlenet   # 登录器 -> 登录 -> 安装并运行你的游戏
scripts/play.sh d2r         # 直接启动 D2R（离线）
scripts/play.sh kill        # 全部停止
```

已经有装好游戏的 CrossOver bottle？`scripts/setup-bottle.sh` 会克隆它（省下 28 GB 的游戏重新下载）。

## Epic Games Launcher

同一个引擎，独立 bottle，不需要额外招数：Epic 的 CEF 在这个构建上 GPU 进程能存活，登录器用原始命令行即可运行。2026-08-29 验证：官方 MSI 无人值守安装（约 30 秒）、登录器界面、登录。

```bash
scripts/create-epic-bottle.sh    # 官方 Epic MSI，无人值守
scripts/play.sh epic             # 登录 -> 安装并游玩
scripts/play.sh epic-kill        # 停止 Epic bottle（关窗口会缩到菜单栏托盘；点 Dock 图标恢复）
```

2026-08-30 验证：从登录器安装 71 GB 的《霍格沃茨之遗》（UE4，D3D12 走 D3DMetal），进入游戏。两点提示：中日韩输入法已经不影响了（engine-v1.5 起像 Windows 一样，用输入法底下的英文布局生成键表，游戏中切到中文再切回来也没问题）；如果游戏以全屏启动，请在游戏自己的显示设置里改成窗口模式（Soju 不强制）。wine 11.0 的一个已知问题：在多个 Wine 窗口（登录器、游戏、另一个 bottle）之间切换几次后，游戏可能收不到键盘输入，重启游戏即可；上游在 wine 11.11 修复。

### GOG GALAXY

```bash
scripts/create-gog-bottle.sh      # 独立 bottle，官方网页安装器（静默）
scripts/play.sh gog               # 登录、安装、游玩
scripts/play.sh gog-kill          # 停止 GOG bottle（关窗口会缩到菜单栏托盘）
```

2026-08-30 验证：安装、登录、游戏库。GOG GALAXY 2.x 是 Qt6 + QtWebEngine 而不是 CEF，在这个引擎上窗口会黑屏：Qt 的 D3D11 合成会向 D3DMetal 的 DXGI 要 `IDXGIResource`，而后者没有实现。解法是让 Chromium 走 CPU 渲染（`--disable-gpu`），但 GOG 会自己覆盖 `QTWEBENGINE_CHROMIUM_FLAGS`、无视自己的命令行参数，还会校验自身可执行文件，引擎外部没有任何注入手段。因此引擎带了一个小钩子（`patches/chromium-flags-append.patch`）：每当程序设置 `QTWEBENGINE_CHROMIUM_FLAGS` 时，把 `SOJU_CHROMIUM_FLAGS` 的内容追加上去。`play.sh gog` 会设置它。同一组补丁还加了 `WINE_CUSTOM_FRAME`，阻止 Mac 驱动在 GOG 自绘标题栏上再叠一个 macOS 标题栏。细节见 [docs/DIAGNOSIS.md](docs/DIAGNOSIS.md)。

游戏尚未大范围测试；需要内核级反作弊（EAC/BattlEye）的游戏在 Wine 下无法运行。

## Steam（包括 Steam 版 D2R）

D2R 于 2026 年 2 月以 *Infernal Edition* 上架 Steam。Steam 支持用的是**另一个免费引擎**：新版 Steam 客户端的 CEF 界面在 CrossOver 源码构建上无法渲染（黑窗口，跨进程交换链 + CEF 沙箱问题），但在 Homebrew 的 `wine-stable` 11 上，配合一个强制 `--disable-gpu --single-process` 的小 `steamwebhelper` 包装器就能工作。该修复来自 [notpop/steam-on-m1-wine](https://github.com/notpop/steam-on-m1-wine)（MIT，包装器源码收录在 `third_party/`）。Steam 有自己独立的 bottle，两套栈互不干扰：

```bash
scripts/create-steam-bottle.sh   # 安装 wine-stable + 包装器 + Valve 官方安装包
scripts/play.sh steam            # 登录 -> 安装并游玩
scripts/play.sh steam-kill       # 停止 Steam bottle
```

在 M4 Pro / macOS 26.5 上验证：登录、游戏库，以及一个真实 D3D11（Unity）游戏通过 DXMT 分支在游戏内渲染。完整接线见 `docs/STEAM-GAMES.md`（`scripts/setup-steam-games.sh`）。注意：在 Steam 里打字时把 macOS 输入法切到英文（ABC），否则输入法合成会显示为 `?`。

**前提条件**：Apple Silicon Mac、Rosetta 2、Xcode 命令行工具、Homebrew、你自己的战网账号/游戏，以及一个用于下载 GPTK 的免费 Apple ID。**《暗黑破坏神 II：狱火重生》需要 macOS 26.4 或更高**：暴雪 2026 年 1 月加入的反作弊会触发一个 Rosetta 2 的 bug，Apple 在 26.4 修复了它。在 macOS 15 上游戏启动后立刻退出（战网随后显示"更新"并卡在正在初始化），CrossOver 也一样。启动器和其他游戏不受这个 bug 影响，但所有验证都只在 macOS 26.5 上做过。本仓库不再分发 Apple、Blizzard 或 CodeWeavers 的任何文件。

### 为什么必须要 GPTK？

`libd3dshared.dylib`（GPTK 内）不只是图形库：D2R 的加载器需要它的*非原生代码区域注册*才能通过 Rosetta 2。没有它，即使已开启 AVX 广播，游戏也会在启动时卡死。图形本身可以在没有 D3DMetal 时用纯开源的 vkd3d/MoltenVK 运行。

### 疑难解答

先跑一遍 `soju doctor`：它会检查下面大部分条目并直接指出问题。

- **弹出 "Wine Mono Installer"** -> 跳过了第 2 步；运行 `get-components.sh` 后重新 `build-engine.sh`。
- **游戏卡在约 86 MB 内存、0% CPU** -> `ROSETTA_ADVERTISE_AVX=1` 或 `libd3dshared` 没有传到游戏。只通过 `play.sh` 启动，并检查第 4 步。
- **找不到库（gnutls/freetype 报错）** -> 你通过 `nohup`/`arch`/其他 Apple 签名的二进制启动了 wine，`DYLD_*` 变量被剥掉了。请通过 `play.sh` 启动。
- **战网登录 webview 偶尔闪烁（约一分钟一次）** -> 已知的外观问题；会自动恢复，不影响登录。
- **虚幻引擎游戏启动时提示 "The installed version of the AMD graphics driver has known issues"**（《霍格沃茨之遗》等）：D3DMetal 把自己伪装成旧驱动版本的 AMD 显卡，触发了 UE4 的驱动检查。无害，点确定即可；想彻底关掉就在游戏的用户 `Engine.ini`（`AppData/Local/<游戏>/Saved/Config/WindowsNoEditor/` 下）加入 `[SystemSettings]` / `r.WarnOfBadDrivers=0`。
- **GOG GALAXY：通知弹出时右下角出现黑色矩形**，随通知一起消失。通知是 GOG 通知渲染器的透明分层窗口；没有 DWM 合成时 Wine 会把透明区域涂黑。无害。在 GOG 设置里关闭桌面通知即可避免。
- **移动键卡在按下状态，或者鼠标正常但键盘完全没反应**（在 Hogwarts Legacy 中出现过）：按住键的时候键盘焦点被另一个 Wine 窗口抢走了。常见元凶是 Epic（EOS）覆盖层，所以 `soju epic` 现在默认禁用它（`SOJU_EPIC_OVERLAY=1` 可恢复；先 `soju epic-kill` 再重启登录器才会生效）。v1.5 之前的引擎还有第二个原因：中日韩输入法会让字母键变成假名或字母以外的字符，虚幻引擎的游戏会直接丢弃（只有 Esc、方向键和 F 键有效），切换瞬间按住的键也会卡住；在那些引擎上请保持英文（ABC），或者更新引擎（`soju update`）。仍然复现？`SOJU_KEYLOG=1 soju epic` 会把按键和焦点日志写到 `~/.battlenet-macos/logs/`，提 issue 时附上。
- **Epic 提示 "your account has too many active logins"**，而且在所有设备退出登录、甚至重置密码后依旧如此。账号服务实际返回的是 `too_many_sessions`（18048），限制的是*已发放*的会话数量而非当前持有的数量，所以反复重试只会更糟。旧引擎上启动几次就会触发，因为 Wine 无法保存登录器的设备密钥，详见 `patches/ncrypt-persisted-keys.patch`。先停掉 bottle（`soju epic-kill`），放置几个小时等计数器清零，然后只启动一次。
- **BLZBNTBNA00000005** -> `play.sh` 会自动播种签名 exe；请确认是通过它启动的。

### 残留的 Wine 进程

每次启动 bottle 都会附带一组空闲的 Windows 服务进程（`services.exe`、`winedevice.exe`、`plugplay.exe`、`rpcss.exe`、`explorer.exe /desktop`，每组约 100 MB）。如果 bottle 的 `wineserver` 被强杀（崩溃、中断的运行），这些服务不会察觉，会一直挂着。`scripts/soju-sweep.sh` 负责清掉它们，`play.sh kill` / `epic-kill` / `steam-kill` 和守护进程会自动调用。它它只清理不属于任何运行中 `wineserver` 的服务进程（存活 bottle 的每个进程都在其服务端 socket 目录里打开着文件），因此正在运行的游戏或登录器绝不会被误伤，即使其他 bottle 还在运行也一样。可用 `soju sweep` 手动执行。

## 仓库里有什么

- `scripts/build-engine.sh`：完整的引擎构建配方（freetype 交叉构建、gnutls 接线、GPTK 布局、entitlements）
- `scripts/setup-bottle.sh` / `scripts/play.sh`：bottle 创建和验证过的启动环境
- `docs/DIAGNOSIS.md`：CEF 渲染进程崩溃与 Agent 签名失败的深度分析
- `docs/D2R-GAME-LAUNCH.md`：D2R 加载器调查全记录，包括死胡同
- `research/`：结论对应的原始日志证据

## 许可

- 本仓库的脚本和文档：**GPL-3.0**（见 LICENSE）
- 引擎构建自 CodeWeavers 公开的 Wine 源码（GPL/LGPL）。来源：`media.codeweavers.com/pub/crossover/source/`
- **不包含且永远不会包含**：Apple D3DMetal/GPTK 二进制（禁止再分发）、CrossOver 应用二进制、任何 Blizzard 文件
- 本项目与 CodeWeavers、Apple、Blizzard Entertainment 均无关联。Battle.net 和 Diablo 是 Blizzard Entertainment 的商标。使用风险自负；使用第三方兼容层进行在线游戏请自行斟酌。
