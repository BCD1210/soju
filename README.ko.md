# Soju (한국어)

> *Wine → Whisky → Kegworks… 그리고 한국의 차례: **Soju** 🍶*

**런처가 실제로 로그인됩니다.** Apple Silicon 맥에서 Battle.net·Steam·Epic Games Launcher·GOG GALAXY. 검은 로그인 창도, 끝나지 않는 "Signing in…"도, CrossOver 라이선스도 없이. 완전 무료 오픈소스 Wine 스택.

Whisky/Kegworks 스레드의 *"배틀넷 로그인 시 크래시"*, *"Steam이 안 열림"*, *"런처 창이 비어 있음"* 때문에 오셨다면, 바로 그 벽들을 이 레포가 기록하고 해결했습니다(CEF 렌더러 `PAGE_WRITECOPY` int3, CEF GPU 프로세스 초기화 사망, Steam webhelper). [docs/DIAGNOSIS.md](docs/DIAGNOSIS.md).

CodeWeavers가 GPL로 공개한 소스(Wine 11.0, CrossOver 26.3 소스 드롭)를 이 레포의 스크립트로 직접 빌드·조립합니다. 유료 소프트웨어 불필요.

> 상태(2026-08): **전 구간 동작**: 배틀넷 로그인·Agent·D2R 인게임 렌더링(D3DMetal), Epic Games Launcher 로그인·게임 설치(메뉴바 트레이), GOG GALAXY 로그인, Steam 로그인 + D3D11 게임. M4 Pro / macOS 26.5에서 검증.

*[English README](README.md) · [简体中文说明](README.zh.md)*

## 핵심 발견 3가지

커뮤니티가 몇 달째 못 풀던 문제들의 해법:

1. **`ROSETTA_ADVERTISE_AVX=1`**: D2R 로더는 AVX 명령어가 필수입니다. 이 환경변수가 없으면 게임이 그래픽 초기화 전에 86MB/0%CPU로 영원히 멈춥니다. "맥에서 D2R이 실행 안 됨"의 정체입니다.
2. **D3DMetal 심링크 레이아웃**: 애플 GPTK 라이브러리는 `lib/external/`에 실물을 두고, `lib/wine/x86_64-unix/`의 d3d10/11/12/dxgi.so는 **심링크**여야 합니다. 복사하면 `@loader_path`가 어긋나 assertion 루프로 죽습니다.
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

설치기는 프리빌드 엔진(~350MB)을 받고, 애플 무료 GPTK 다운로드(애플이 재배포 금지한 파일 1개)를 안내한 뒤, 원하는 런처를 묻습니다. Battle.net, Steam, Epic Games Launcher, GOG GALAXY 중 아무 조합이나 고르면 각각 공식 설치기로 별도 보틀에 설치하고, 런처마다 더블클릭용 앱을 `~/Applications`에 만들어 줍니다(`Battle.net.app`, `Steam (Windows).app`, `Epic Games Launcher.app`, `GOG GALAXY.app`). 비대화형: `SOJU_PLATFORMS=battlenet,epic,gog curl ... | bash`. 로그인하고 플레이하세요.

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
#    dmg 다운로드 후 마운트한 상태로:)
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

검증 2026-08-30: 런처에서 Hogwarts Legacy 71GB 설치 후 인게임까지(UE4, D3DMetal 경유 D3D12). 알아두실 점 두 가지: 플레이 전에 macOS 입력기를 영어(ABC)로 바꾸세요, 한글 IME가 켜져 있으면 Wine 게임에 키 입력이 전달되지 않습니다. 게임이 전체화면으로 뜨면 게임 자체 디스플레이 설정에서 창 모드로 바꾸면 됩니다(Soju가 강제하지 않습니다). wine 11.0의 알려진 문제 하나: 여러 Wine 창(런처, 게임, 다른 보틀)을 오간 뒤 게임이 키보드 입력을 못 받는 경우가 있으며 재시작해야 풀립니다. upstream은 wine 11.11에서 수정했습니다.

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

**전제조건**: Apple Silicon 맥, Rosetta 2, Xcode CLT, Homebrew, 본인 배틀넷 계정, GPTK용 무료 Apple ID.

### GPTK가 왜 필요한가?

GPTK 안의 `libd3dshared.dylib`는 그래픽만이 아닙니다. **D2R 로더(안티치트)가 Rosetta 2를 통과하려면 이 파일의 '비네이티브 코드영역 등록' 기능이 필수**입니다. 없으면 AVX를 켜도 실행 직후 멈춥니다. 그래픽 자체는 D3DMetal 없이 순수 오픈소스 vkd3d/MoltenVK로도 돌아갑니다.

### 문제 해결

- **"Wine Mono Installer" 팝업** → 2단계가 생략된 것입니다. `get-components.sh` 후 `build-engine.sh`를 다시 실행하세요.
- **게임이 86MB/0% CPU로 영원히 멈춤** → AVX 변수 또는 libd3dshared가 게임에 전달되지 않은 것입니다. 반드시 `play.sh`로 실행하고 4단계를 확인하세요.
- **라이브러리 로드 실패(gnutls/freetype)** → `nohup`/`arch` 등 애플 서명 바이너리를 거치면 `DYLD_*`가 제거됩니다. `play.sh`로 실행하세요.
- **배틀넷 로그인 화면이 가끔 깜빡임(~1분 1회)** → 알려진 외관 이슈이며 자동으로 복구됩니다.
- **언리얼 엔진 게임 시작 시 "AMD graphics driver has known issues" 경고**(Hogwarts Legacy 등): D3DMetal이 자신을 구형 드라이버의 AMD 카드로 소개해 UE4의 드라이버 검사가 걸리는 것입니다. 무해하며 OK로 넘어가면 됩니다. 없애려면 게임의 사용자 `Engine.ini`(`AppData/Local/<게임>/Saved/Config/WindowsNoEditor/`)에 `[SystemSettings]` / `r.WarnOfBadDrivers=0`을 추가하세요.
- **GOG GALAXY: 알림 토스트가 뜨는 동안 오른쪽 아래가 검은 사각형으로 가려짐**, 토스트가 사라지면 함께 사라짐. 토스트는 GOG 알림 렌더러의 투명 레이어 창인데 Wine에는 DWM 합성이 없어 투명 영역이 검게 칠해집니다. 무해하며, GOG 설정에서 데스크톱 알림을 끄면 안 뜹니다.
- **Epic: "your account has too many active logins"**, 모든 기기에서 로그아웃하거나 비밀번호를 재설정해도 그대로인 경우. 서버가 실제로 답하는 것은 `too_many_sessions`(18048)이고, 이는 보유 중인 세션이 아니라 *발급된* 세션 수에 대한 제한이라 재시도할수록 나빠진다. 예전 엔진에서는 런처의 기기 키를 wine이 저장하지 못해 몇 번만 실행해도 걸렸다(`patches/ncrypt-persisted-keys.patch` 참고). 보틀을 끄고(`soju epic-kill`) 몇 시간 두어 카운터가 비워진 뒤 한 번만 실행하세요.

- **BLZBNTBNA00000005** → `play.sh`가 서명된 exe를 자동으로 넣어 줍니다.

## 라이선스

- 이 레포의 스크립트·문서: **GPL-3.0**
- 엔진은 CodeWeavers 공개 Wine 소스(GPL/LGPL)로 빌드. 소스: `media.codeweavers.com/pub/crossover/source/`
- 미포함(앞으로도): 애플 D3DMetal/GPTK(재배포 금지), CrossOver 앱 바이너리, 블리자드 파일 일체
- 본 프로젝트는 CodeWeavers·Apple·Blizzard와 무관합니다. Battle.net과 Diablo는 Blizzard Entertainment의 상표입니다.
