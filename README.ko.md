# Soju (한국어)

**1.6.0 추가:** Steam 공식 로그인으로 보유 게임 가져오기(API 키 불필요), GOG Galaxy 로컬 보유 목록 가져오기, Steam·GOG 상품/가격 검색과 공식 스토어 연결. [설정 안내](docs/ACCOUNTS.md).
**v1.6.0:** 설치된 게임을 한 라이브러리에서 검색·필터·즐겨찾기하고 실행할 수 있습니다. 런처 설치와 업데이트는 Platforms에서 관리합니다. Steam·GOG 보유 게임을 가져오고 Discover에서 상품과 가격을 검색할 수 있습니다. Epic·Battle.net 전체 보유 목록 연동은 아직 지원하지 않습니다.

[Desktop / download](docs/DESKTOP.md)

> **Apple Silicon 맥용 무료 오픈소스 게임 런처 도구.**

맥에서 **Battle.net·Steam·Epic Games Launcher·GOG GALAXY**를 설치하고 실행하세요.
Soju는 터미널 설치 도구, 런처별 독립 Wine 환경, 더블클릭으로 실행하는 맥 앱을 제공합니다.
기존 스토어 계정으로 로그인해 지원되는 Windows 게임을 플레이할 수 있습니다.

게임 라이브러리는 각 스토어의 런처에서 이용합니다. 현재 Soju는 CLI와 런처별 실행 앱으로
구성되어 있으며, 게임과 시스템에 따라 호환성이 달라집니다.

도구는 무료 오픈소스입니다. 기본 Wine 엔진은 CodeWeavers 공개 소스로 빌드하며,
Steam은 별도의 wine-stable 환경을 사용합니다. Apple의 비공개 GPTK/D3DMetal 구성요소는
별도로 내려받아야 합니다. 게임과 스토어 클라이언트는 포함하지 않습니다.

**현재 스크립트: [v1.3.6](https://github.com/BCD1210/soju/releases/tag/v1.3.6).**
아래 실행 사례는 M4 Pro / macOS 26.5에서 확인한 결과입니다.
디아블로2와 호그와트 레거시는 지원 사례이며, 프로젝트의 범위는 여러 게임 런처입니다.

[30초 실제 게임 플레이 영상 보기](https://soju.snack-wrap.com/#demo) · 호그와트 레거시 / Epic + Guilt Free / Steam (별도 DXMT 설정)

*[English README](README.md) · [简体中文说明](README.zh.md)*

## 되는 것

<!-- 스크린샷: docs/images/ 에 PNG(d2r-ingame.png, battlenet-login.png, hogwarts-ingame.png)를 넣고 주석을 푸세요.
<p align="center">
  <img src="docs/images/d2r-ingame.png" width="49%" alt="M4 Pro에서 실행 중인 Diablo II: Resurrected">
  <img src="docs/images/battlenet-login.png" width="49%" alt="Apple Silicon에서 로그인된 Battle.net">
</p>
-->

모두 M4 Pro / macOS 26.5에서 검증. 런처마다 별도 보틀을 사용하며, Steam은 엔진도 별도입니다.

| 런처 | 로그인 | 게임 설치 | 비고 |
| --- | :-: | :-: | --- |
| Battle.net | ✅ | ✅ | Agent 동작, `BLZBNTBNA00000005`는 서명 exe 시딩으로 해결 |
| Epic Games Launcher | ✅ | ✅ | 창을 닫으면 메뉴바 트레이로 |
| GOG GALAXY | ✅ | ✅ | 로그인·라이브러리까지, 게임은 아직 폭넓게 테스트 안 됨 |
| Steam (Windows 클라이언트) | ✅ | ✅ | 별도 `wine-stable` + DXMT 보틀 |

| 게임 | 스토어 | 그래픽 | 상태 | 비고 |
| --- | --- | --- | :-: | --- |
| Diablo II: Resurrected | Battle.net | D3DMetal (D3D11) | ✅ 인게임, 온라인 | **macOS 26.4 이상**(전제조건 참고); `play.sh`가 `ROSETTA_ADVERTISE_AVX=1` 설정; [가이드](https://bcd1210.github.io/soju/guides/ko/diablo-2-mac.html) |
| Hogwarts Legacy | Epic | D3DMetal (D3D12) | ✅ 인게임 | UE4 "AMD 드라이버" 경고는 무해, 입력 소스를 영어로 |
| Unity D3D11 타이틀 | Steam | DXMT | ✅ 인게임, 창 모드 | `soju steam-games` 필요, [docs/STEAM-GAMES.md](docs/STEAM-GAMES.md) |
| Diablo II: Resurrected (Infernal Edition) | Steam | — | ⏳ 미검증 | Steam 보틀에는 GPTK가 없고 D2R 로더는 `libd3dshared`가 필요 |

안 되는 것: 커널 안티치트(EAC, BattlEye, Vanguard)가 붙은 게임 전부. 다른 게임을 돌리셨다면 [Discussions](https://github.com/BCD1210/soju/discussions)에 올리거나 표에 한 줄 추가하는 PR을 보내주세요.

## 핵심 발견 3가지

설치와 실행을 안정화하면서 확인한 원인과 해결 방법:

1. **`ROSETTA_ADVERTISE_AVX=1`**: D2R 로더는 AVX 명령어가 필수입니다. 이 환경변수가 없으면 게임이 그래픽 초기화 전에 86MB/0%CPU로 영원히 멈춥니다. "맥에서 D2R이 실행 안 됨"의 정체입니다.
2. **D3DMetal 페이로드 레이아웃**: 애플 GPTK 페이로드는 세 부분이고 셋이 다 있어야 D3DMetal이 붙습니다. `lib/external/`(libd3dshared + D3DMetal.framework), Wine 자체 DLL을 대체하는 애플의 PE shim `d3d11.dll`·`d3d12.dll`·`dxgi.dll`(+ `atidxx64`, `nvapi64`, `nvngx`)을 `lib/wine/x86_64-windows/`에, 그리고 shim마다 `lib/wine/x86_64-unix/<shim>.so`를 `lib/external/`로 가는 **심링크**로. 실물을 복사하면 `@loader_path`가 어긋나 assertion 루프로 죽고, Wine 자체 `d3d11.dll`을 그대로 두면 D3D가 wined3d로 돌아가며 Epic Games Launcher는 시작 직후 크래시합니다. `soju gptk`가 세 부분을 모두 설치합니다.
3. **Battle.net Agent 서명검증 수정**: Agent는 접속한 클라이언트의 서명을 자기 작업폴더(버전 하위폴더) 기준 상대 파일명으로 검사합니다. 서명된 `Battle.net.exe` 사본을 각 `Battle.net.NNNNN` 폴더에 넣으면 통과합니다 (에러 `BLZBNTBNA00000005` 해결).

함정 하나: 실행 체인에 애플 보호 바이너리(`nohup`, `arch` 등)를 두면 macOS가 `DYLD_*` 변수를 제거해 라이브러리를 못 찾습니다.

**가이드:** [맥에서 디아2 돌리기](https://bcd1210.github.io/soju/guides/ko/diablo-2-mac.html) · [영문 가이드 모음](https://bcd1210.github.io/soju/)

## 설치 (한 줄)

```bash
curl -fsSL soju.snack-wrap.com/install.sh | bash
```

Homebrew로도 됩니다:

```bash
brew install BCD1210/soju/soju
soju install     # 이후: soju battlenet / soju d2r / soju steam / soju epic / soju gog
```

설치기는 **다운로드 전에 원하는 런처부터 선택**합니다. Steam만 선택하면 CX 엔진과 애플 GPTK를 받지 않습니다. Steam 그래픽 구성요소는 SHA-256으로 검증한 뒤 Soju 전용 Wine 11.0 환경에 자동으로 설치합니다. Battle.net·Epic·GOG는 공통 CX 엔진과 사용자가 별도로 받은 애플 GPTK를 사용합니다. 각 런처의 보틀·로그인·게임은 독립적으로 유지합니다.

[Soju 맥 앱 다운로드](https://github.com/BCD1210/soju/releases/download/v1.4.0/Soju-1.4.0-macos-arm64.zip) — 네이티브 앱 프리뷰에서 설치·실행·구성요소 업데이트·진단을 한 화면에서 관리할 수 있습니다. [설치 안내와 요구 사항](docs/DESKTOP.md)을 확인해 주세요.

## 일상 명령

```bash
soju doctor      # 스택 전체 점검, 뭐가 문제인지 출력 (이슈 올릴 때 이 출력을 붙여주세요)
soju update      # 스크립트와 엔진 업데이트 (GPTK와 보틀은 그대로 둠)
soju uninstall   # 앱·보틀·엔진 제거, 항목마다 확인 후 진행
```

## 직접 빌드하고 싶다면: CrossOver 불필요

이 레포의 스크립트 밖에서 받는 것은 애플의 무료 Game Porting Toolkit 파일 하나뿐입니다 (무료 Apple ID 필요. 애플이 재배포를 금지해서 직접 받아야 합니다).

```bash
# 1. 스크립트 받기
git clone https://github.com/BCD1210/soju.git && cd soju

# 2. 런타임 구성요소 확보 (x86_64 dylib + wine-mono, 전부 무료 GPL 릴리스에서)
scripts/get-components.sh

# 3. GPL 소스에서 엔진 빌드 (30~60분)
scripts/build-engine.sh

# 4. 애플 GPTK 설치 (https://developer.apple.com/games/game-porting-toolkit/ 에서
#    "evaluation environment for Windows games" dmg를 받아 마운트한 상태로:)
scripts/get-gptk.sh
#    (CrossOver가 설치돼 있으면 거기서 자동 추출)

# 5. 보틀 생성 + Battle.net 설치 (블리자드 공식 설치기, 완전 자동)
scripts/create-bottle.sh

# 6. 플레이
scripts/play.sh battlenet   # 런처 → 로그인 → 게임 설치·플레이 (창을 닫으면 Windows처럼 종료. 설정에서 "트레이로 최소화"로 바꾸면 독 아이콘 클릭으로 복귀)
scripts/play.sh d2r         # 게임 직접 실행 (오프라인)
scripts/play.sh kill        # 전부 종료
```

이미 CrossOver 보틀에 게임이 설치돼 있다면 `scripts/setup-bottle.sh`로 복제하세요 (28GB 재다운로드 회피).

## Epic Games Launcher

같은 엔진, 별도 보틀, 추가 트릭 없음: Epic의 CEF는 이 빌드에서 GPU 프로세스가 죽지 않아 런처가 기본 커맨드라인 그대로 뜹니다. 검증 2026-08-29: 공식 MSI 무인 설치(~30초), 런처 UI, 로그인.

```bash
scripts/create-epic-bottle.sh    # 공식 Epic MSI, 무인 설치
scripts/play.sh epic             # 로그인 → 게임 설치·플레이
scripts/play.sh epic-kill        # Epic 보틀 종료 (창을 닫으면 런처는 메뉴바 트레이로 들어감. 독 아이콘 클릭, 또는 메뉴바 아이콘 더블클릭/우클릭으로 복귀)
```

검증 2026-08-30: 런처에서 Hogwarts Legacy 71GB 설치 후 인게임까지(UE4, D3DMetal 경유 D3D12). 알아두실 점 두 가지: 한글/일본어/중국어 입력기는 이제 상관없습니다(engine-v1.5부터 Windows처럼 입력기 밑의 영문 레이아웃으로 키 테이블을 만들어서, 게임 중 한글로 바꿨다 돌아와도 됩니다). 게임이 전체화면으로 뜨면 게임 자체 디스플레이 설정에서 창 모드로 바꾸면 됩니다(Soju가 강제하지 않습니다). wine 11.0의 알려진 문제 하나: 여러 Wine 창(런처, 게임, 다른 보틀)을 오간 뒤 게임이 키보드 입력을 못 받는 경우가 있으며 재시작해야 풀립니다. upstream은 wine 11.11에서 수정했습니다.

### GOG GALAXY

```bash
scripts/create-gog-bottle.sh      # 별도 보틀, 공식 웹 설치기(무인)
scripts/play.sh gog               # 로그인 → 게임 설치·플레이
scripts/play.sh gog-kill          # GOG 보틀 종료 (창을 닫으면 메뉴바 트레이로 들어가고, 독 아이콘 클릭으로 돌아옴)
```

검증 2026-08-30: 설치, 로그인, 라이브러리. GOG GALAXY 2.x는 CEF가 아니라 Qt6 + QtWebEngine이며, 이 엔진에서는 Qt의 D3D11 합성이 D3DMetal DXGI에 없는 `IDXGIResource`를 요구해 창이 검게 나옵니다. 해법은 Chromium을 CPU로 돌리는 것(`--disable-gpu`)인데, GOG가 `QTWEBENGINE_CHROMIUM_FLAGS`를 스스로 덮어쓰고 자기 커맨드라인도 무시하며 실행 파일 체크섬까지 검사해서 엔진 밖에서는 스위치를 넣을 방법이 없습니다. 그래서 엔진에 작은 훅(`patches/chromium-flags-append.patch`)을 넣었습니다: 프로그램이 `QTWEBENGINE_CHROMIUM_FLAGS`를 설정할 때마다 `SOJU_CHROMIUM_FLAGS`의 내용이 덧붙습니다. `play.sh gog`가 이 값을 설정합니다. 같은 패치에 `WINE_CUSTOM_FRAME`도 추가해, GOG 자체 타이틀바 위에 macOS 타이틀바가 겹치지 않게 했습니다. 자세한 내용은 [docs/DIAGNOSIS.md](docs/DIAGNOSIS.md).


게임은 아직 폭넓게 테스트하지 않았습니다. 커널 안티치트(EAC/BattlEye)가 필요한 게임은 Wine에서 돌지 않습니다.

## Steam 지원 (Steam판 D2R 포함)

D2R은 2026년 2월 Steam에도 *Infernal Edition*으로 출시됐습니다. Steam은 **다른 무료 엔진**을 씁니다: 최신 Steam 클라이언트의 CEF UI는 CrossOver 소스 계열 빌드에서 렌더링되지 않지만(검은 창, 크로스 프로세스 스왑체인 + CEF 샌드박스 문제), homebrew `wine-stable` 11 + `steamwebhelper` 래퍼(`--disable-gpu --single-process` 강제)로는 동작합니다. 이 해법은 [notpop/steam-on-m1-wine](https://github.com/notpop/steam-on-m1-wine)(MIT, 래퍼 소스는 `third_party/`에 동봉)에서 왔습니다. Steam은 별도 보틀이라 두 스택이 서로 간섭하지 않습니다:

```bash
scripts/create-steam-bottle.sh   # wine-stable + 래퍼 + Valve 공식 설치기
scripts/play.sh steam            # 로그인 → 게임 설치·플레이
scripts/play.sh steam-kill       # Steam 보틀 종료
```

검증: M4 Pro / macOS 26.5. 로그인·라이브러리·실제 D3D11(Unity) 게임 인게임 렌더링까지 (DXMT 포크). 전체 배선은 `docs/STEAM-GAMES.md` + `scripts/setup-steam-games.sh` 참고. 참고: Steam 입력 시 macOS 입력기를 영어(ABC)로 전환하세요 (한글 IME 조합이 `?`로 보입니다).

**전제조건**: Apple Silicon 맥, Rosetta 2, Xcode CLT, Homebrew, 본인 배틀넷 계정, GPTK용 무료 Apple ID. **Diablo II: Resurrected는 macOS 26.4 이상이 필요합니다**: 2026년 1월 블리자드가 넣은 안티치트가 Rosetta 2 버그를 건드리는데 Apple이 26.4에서 고쳤습니다. macOS 15에서는 게임이 실행 직후 죽고(배틀넷은 "업데이트"를 띄운 뒤 Initializing에 멈춤) CrossOver도 마찬가지입니다. 런처와 다른 게임은 이 버그와 무관하지만, 검증은 전부 macOS 26.5에서만 했습니다.

### GPTK가 왜 필요한가?

GPTK 안의 `libd3dshared.dylib`는 그래픽만이 아닙니다. **D2R 로더(안티치트)가 Rosetta 2를 통과하려면 이 파일의 '비네이티브 코드영역 등록' 기능이 필수**입니다. 없으면 AVX를 켜도 실행 직후 멈춥니다. 그래픽 자체는 D3DMetal 없이 순수 오픈소스 vkd3d/MoltenVK로도 돌아갑니다.

### 문제 해결

- **"Wine Mono Installer" 팝업** → 2단계가 생략된 것입니다. `get-components.sh` 후 `build-engine.sh`를 다시 실행하세요.
- **게임이 86MB/0% CPU로 영원히 멈춤** → AVX 변수 또는 libd3dshared가 게임에 전달되지 않은 것입니다. 반드시 `play.sh`로 실행하고 4단계를 확인하세요.
- **라이브러리 로드 실패(gnutls/freetype)** → `nohup`/`arch` 등 애플 서명 바이너리를 거치면 `DYLD_*`가 제거됩니다. `play.sh`로 실행하세요.
- **배틀넷 로그인 화면이 가끔 깜빡임(~1분 1회)** → 알려진 외관 이슈이며 자동으로 복구됩니다.
- **언리얼 엔진 게임 시작 시 "AMD graphics driver has known issues" 경고**(Hogwarts Legacy 등): D3DMetal이 자신을 구형 드라이버의 AMD 카드로 소개해 UE4의 드라이버 검사가 걸리는 것입니다. 무해하며 OK로 넘어가면 됩니다. 없애려면 게임의 사용자 `Engine.ini`(`AppData/Local/<게임>/Saved/Config/WindowsNoEditor/`)에 `[SystemSettings]` / `r.WarnOfBadDrivers=0`을 추가하세요.
- **GOG GALAXY: 알림 토스트가 뜨는 동안 오른쪽 아래가 검은 사각형으로 가려짐**, 토스트가 사라지면 함께 사라짐. 토스트는 GOG 알림 렌더러의 투명 레이어 창인데 Wine에는 DWM 합성이 없어 투명 영역이 검게 칠해집니다. 무해하며, GOG 설정에서 데스크톱 알림을 끄면 안 뜹니다.
- **이동키가 눌린 채로 고정되거나, 마우스는 되는데 키보드가 죽는 경우** (Hogwarts Legacy에서 확인): 키를 누른 사이에 키보드 포커스가 다른 Wine 창으로 넘어간 것입니다. 주범은 Epic(EOS) 오버레이라 `soju epic`은 이제 오버레이를 끕니다(`SOJU_EPIC_OVERLAY=1`로 복구, 적용하려면 `soju epic-kill` 후 재실행). v1.5 이전 엔진에서는 한글/일본어/중국어 입력기가 두 번째 원인입니다: 글자키가 자모나 가나로 번역돼 언리얼 게임이 버리고(ESC·방향키·F키만 됨), 전환 순간 누르고 있던 키는 고정됩니다. 그 엔진에서는 입력기를 영어(ABC)로 두거나 엔진을 업데이트하세요(`soju update`). 그래도 재발하면 `SOJU_KEYLOG=1 soju epic`이 `~/.battlenet-macos/logs/`에 키·포커스 로그를 남기니 이슈에 첨부해 주세요.
- **Epic: "your account has too many active logins"**, 모든 기기에서 로그아웃하거나 비밀번호를 재설정해도 그대로인 경우. 서버가 실제로 답하는 것은 `too_many_sessions`(18048)이고, 이는 보유 중인 세션이 아니라 *발급된* 세션 수에 대한 제한이라 재시도할수록 나빠진다. 예전 엔진에서는 런처의 기기 키를 wine이 저장하지 못해 몇 번만 실행해도 걸렸다(`patches/ncrypt-persisted-keys.patch` 참고). 보틀을 끄고(`soju epic-kill`) 몇 시간 두어 카운터가 비워진 뒤 한 번만 실행하세요.

- **게임 종료 후(또는 실행이 죽은 뒤) 배틀넷이 "업데이트"(초기화 중)에 멈추고 플레이 버튼이 사라짐**: 게임 프로세스가 끝날 때마다 런처가 설치를 다시 점검합니다. 업데이트를 일시정지했다가 재개하면 플레이 버튼이 돌아오고 정상 실행됩니다. 설치 후 첫 플레이가 1분 안에 죽으면 플레이를 한 번 더 누르세요: 한 번 보고된 사례에서 두 번째 실행은 정상이었습니다.
- **BLZBNTBNA00000005** → `play.sh`가 서명된 exe를 자동으로 넣어 줍니다.

## 라이선스

- 이 레포의 스크립트·문서: **GPL-3.0**
- 엔진은 CodeWeavers 공개 Wine 소스(GPL/LGPL)로 빌드. 소스: `media.codeweavers.com/pub/crossover/source/`
- 미포함(앞으로도): 애플 D3DMetal/GPTK(재배포 금지), CrossOver 앱 바이너리, 블리자드 파일 일체
- 본 프로젝트는 CodeWeavers·Apple·Blizzard와 무관합니다. Battle.net과 Diablo는 Blizzard Entertainment의 상표입니다.

## 게임 호환성 결과 제보

맥에서 게임을 실행해 보셨나요? 성공·부분 작동·실패 결과를 게임, 스토어, 맥 모델, macOS, Soju 설정과 함께 남겨 주세요. GitHub 로그인이 필요하며 제보 내용은 공개됩니다.

[게임 호환성 결과 제보](https://github.com/BCD1210/soju/issues/new?template=compatibility.yml)
