# Soju (한국어)

> *Wine → Whisky → Kegworks… 그리고 한국의 차례: **Soju** 🍶*

**Apple Silicon 맥에서 Battle.net·디아블로 II: 레저렉션·Epic Games Launcher·Steam을 — 완전 무료 오픈소스 Wine 스택으로.**

CodeWeavers가 GPL로 공개한 소스(Wine 11.0, CrossOver 26.3 소스 드롭)를 이 레포의 스크립트로 직접 빌드·조립합니다. 유료 소프트웨어 불필요.

> 상태(2026-08): **전 구간 동작** — 배틀넷 로그인, Agent, D2R 인게임 렌더링(D3DMetal)까지. M4 Pro / macOS 26.5에서 검증.

*[English README](README.md)*

## 핵심 발견 3가지 🔑

커뮤니티가 몇 달째 못 풀던 문제들의 해법:

1. **`ROSETTA_ADVERTISE_AVX=1`** — D2R 로더는 AVX 명령어가 필수. 이 환경변수가 없으면 게임이 그래픽 초기화 전에 86MB/0%CPU로 영원히 멈춘다. "맥에서 D2R이 실행 안 됨"의 정체.
2. **D3DMetal 심링크 레이아웃** — 애플 GPTK 라이브러리는 `lib/external/`에 실물을 두고, `lib/wine/x86_64-unix/`의 d3d10/11/12/dxgi.so는 **심링크**여야 한다. 복사하면 `@loader_path`가 어긋나 assertion 루프로 죽는다.
3. **Battle.net Agent 서명검증 수정** — Agent는 접속한 클라이언트의 서명을 자기 작업폴더(버전 하위폴더) 기준 상대 파일명으로 검사한다. 서명된 `Battle.net.exe` 사본을 각 `Battle.net.NNNNN` 폴더에 넣으면 통과 (에러 `BLZBNTBNA00000005` 해결).

함정 하나: 실행 체인에 애플 보호 바이너리(`nohup`, `arch` 등)를 두면 macOS가 `DYLD_*` 변수를 제거해 라이브러리를 못 찾는다.

**가이드:** [맥에서 디아2 돌리기](https://bcd1210.github.io/soju/guides/ko/diablo-2-mac.html) · [영문 가이드 모음](https://bcd1210.github.io/soju/)

## 설치 (한 줄)

```bash
curl -fsSL https://raw.githubusercontent.com/BCD1210/soju/main/install.sh | bash
```

Homebrew로도 됩니다:

```bash
brew install BCD1210/soju/soju
soju install     # 이후: soju battlenet / soju d2r / soju steam
```

프리빌드 엔진(~350MB)을 받고, 애플 무료 GPTK 다운로드(애플이 재배포 금지한 파일 1개)를 안내하고, 블리자드 공식 설치기로 Battle.net을 자동 설치한 뒤 `~/Applications/Battle.net.app`을 만들어줍니다. 로그인하고 플레이하세요.

## 직접 빌드하고 싶다면 — CrossOver 불필요

이 레포의 스크립트 밖에서 받는 것은 애플의 무료 Game Porting Toolkit 파일 하나뿐입니다 (무료 Apple ID 필요 — 애플이 재배포를 금지해서 직접 받아야 합니다).

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
scripts/play.sh battlenet   # 런처 → 로그인 → 게임 설치·플레이
scripts/play.sh d2r         # 게임 직접 실행 (오프라인)
scripts/play.sh kill        # 전부 종료
```

이미 CrossOver 보틀에 게임이 설치돼 있다면 `scripts/setup-bottle.sh`로 복제하세요 (28GB 재다운로드 회피).

## Epic Games Launcher

같은 엔진, 별도 보틀, 추가 트릭 없음: Epic의 CEF는 이 빌드에서 GPU 프로세스가 죽지 않아 런처가 기본 커맨드라인 그대로 뜹니다. 검증 2026-08-29: 공식 MSI 무인 설치(~30초), 런처 UI, 로그인.

```bash
scripts/create-epic-bottle.sh    # 공식 Epic MSI, 무인 설치
scripts/play.sh epic             # 로그인 → 게임 설치·플레이
scripts/play.sh epic-kill        # Epic 보틀 종료 (창을 닫아도 런처는 살아 있음 — macOS 메뉴바의 Epic 아이콘을 **우클릭**해 열기/Exit, Windows 트레이와 동일 — 좌클릭 한 번엔 반응 없음)
```

게임은 아직 폭넓게 테스트하지 않았습니다. 커널 안티치트(EAC/BattlEye)가 필요한 게임은 Wine에서 돌지 않습니다.

## Steam 지원 (Steam판 D2R 포함)

D2R은 2026년 2월 Steam에도 *Infernal Edition*으로 출시됐습니다. Steam은 **다른 무료 엔진**을 씁니다: 최신 Steam 클라이언트의 CEF UI는 CrossOver 소스 계열 빌드에서 렌더링되지 않지만(검은 창 — 크로스 프로세스 스왑체인 + CEF 샌드박스 문제), homebrew `wine-stable` 11 + `steamwebhelper` 래퍼(`--disable-gpu --single-process` 강제)로는 동작합니다. 이 해법은 [notpop/steam-on-m1-wine](https://github.com/notpop/steam-on-m1-wine)(MIT, 래퍼 소스는 `third_party/`에 동봉)에서 왔습니다. Steam은 별도 보틀이라 두 스택이 서로 간섭하지 않습니다:

```bash
scripts/create-steam-bottle.sh   # wine-stable + 래퍼 + Valve 공식 설치기
scripts/play.sh steam            # 로그인 → 게임 설치·플레이
scripts/play.sh steam-kill       # Steam 보틀 종료
```

검증: M4 Pro / macOS 26.5 — 로그인·라이브러리·실제 D3D11(Unity) 게임 인게임 렌더링까지 (DXMT 포크). 전체 배선은 `docs/STEAM-GAMES.md` + `scripts/setup-steam-games.sh` 참고. 참고: Steam 입력 시 macOS 입력기를 영어(ABC)로 전환하세요 (한글 IME 조합이 `?`로 보입니다).

**전제조건**: Apple Silicon 맥, Rosetta 2, Xcode CLT, Homebrew, 본인 배틀넷 계정, GPTK용 무료 Apple ID.

### GPTK가 왜 필요한가?

GPTK 안의 `libd3dshared.dylib`는 그래픽만이 아닙니다 — **D2R 로더(안티치트)가 Rosetta 2를 통과하려면 이 파일의 '비네이티브 코드영역 등록' 기능이 필수**입니다. 없으면 AVX를 켜도 실행 직후 멈춥니다. 그래픽 자체는 D3DMetal 없이 순수 오픈소스 vkd3d/MoltenVK로도 돌아갑니다.

### 문제 해결

- **"Wine Mono Installer" 팝업** → 2단계 생략됨. `get-components.sh` 후 `build-engine.sh` 재실행.
- **게임이 86MB/0% CPU로 영원히 멈춤** → AVX 변수 또는 libd3dshared가 게임에 전달되지 않음. 반드시 `play.sh`로 실행하고 4단계 확인.
- **라이브러리 로드 실패(gnutls/freetype)** → `nohup`/`arch` 등 애플 서명 바이너리를 거치면 `DYLD_*`가 제거됨. `play.sh`로 실행.
- **배틀넷 로그인 화면이 가끔 깜빡임(~1분 1회)** → 알려진 외관 이슈, 자동 복구됨.
- **BLZBNTBNA00000005** → `play.sh`가 서명 exe를 자동 시드함.

## 라이선스

- 이 레포의 스크립트·문서: **GPL-3.0**
- 엔진은 CodeWeavers 공개 Wine 소스(GPL/LGPL)로 빌드 — 소스: `media.codeweavers.com/pub/crossover/source/`
- 미포함(앞으로도): 애플 D3DMetal/GPTK(재배포 금지), CrossOver 앱 바이너리, 블리자드 파일 일체
- 본 프로젝트는 CodeWeavers·Apple·Blizzard와 무관합니다. Battle.net과 Diablo는 Blizzard Entertainment의 상표입니다.
