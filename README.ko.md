# Soju (한국어)

> *Wine → Whisky → Kegworks… 그리고 한국의 차례: **Soju** 🍶*

**Apple Silicon 맥에서 Battle.net과 디아블로 II: 레저렉션을 — 완전 무료 오픈소스 Wine 스택으로.**

CodeWeavers가 GPL로 공개한 소스(Wine 11.0, CrossOver 26.3 소스 드롭)를 이 레포의 스크립트로 직접 빌드·조립합니다. 유료 소프트웨어 불필요.

> 상태(2026-08): **전 구간 동작** — 배틀넷 로그인, Agent, D2R 인게임 렌더링(D3DMetal)까지. M4 Pro / macOS 26.5에서 검증.

*[English README](README.md)*

## 핵심 발견 3가지 🔑

커뮤니티가 몇 달째 못 풀던 문제들의 해법:

1. **`ROSETTA_ADVERTISE_AVX=1`** — D2R 로더는 AVX 명령어가 필수. 이 환경변수가 없으면 게임이 그래픽 초기화 전에 86MB/0%CPU로 영원히 멈춘다. "맥에서 D2R이 실행 안 됨"의 정체.
2. **D3DMetal 심링크 레이아웃** — 애플 GPTK 라이브러리는 `lib/external/`에 실물을 두고, `lib/wine/x86_64-unix/`의 d3d10/11/12/dxgi.so는 **심링크**여야 한다. 복사하면 `@loader_path`가 어긋나 assertion 루프로 죽는다.
3. **Battle.net Agent 서명검증 수정** — Agent는 접속한 클라이언트의 서명을 자기 작업폴더(버전 하위폴더) 기준 상대 파일명으로 검사한다. 서명된 `Battle.net.exe` 사본을 각 `Battle.net.NNNNN` 폴더에 넣으면 통과 (에러 `BLZBNTBNA00000005` 해결).

함정 하나: 실행 체인에 애플 보호 바이너리(`nohup`, `arch` 등)를 두면 macOS가 `DYLD_*` 변수를 제거해 라이브러리를 못 찾는다.

## 빠른 시작 (처음부터)

**0단계 — 구성요소 1회 수집.** [CrossOver 무료 체험판](https://www.codeweavers.com/crossover)을 설치하고, 그 안에서 배틀넷과 게임을 설치하세요 (이 과정에서 빌드에 필요한 D3DMetal·wine-mono·x86_64 dylib도 함께 확보됩니다). CrossOver는 이때 한 번만 필요하고, 세팅 후 삭제해도 됩니다.

```bash
# 1. 스크립트 받기
git clone https://github.com/BCD1210/soju.git && cd soju

# 2. GPL 소스에서 엔진 빌드 (30~60분, 소스 ~150MB 다운로드)
scripts/build-engine.sh

# 3. 보틀 생성 (CrossOver의 배틀넷 보틀을 레지스트리째 복제)
DEST=~/.battlenet-macos/bottle scripts/setup-bottle.sh

# 4. 플레이
scripts/play.sh battlenet   # 런처 → 로그인 → Play
scripts/play.sh d2r         # 게임 직접 실행 (오프라인)
scripts/play.sh kill        # 전부 종료
```

더블클릭 앱을 원하면 Automator "셸 스크립트 실행"으로 `play.sh battlenet`을 감싸면 됩니다.

### 문제 해결

- **"Wine Mono Installer" 팝업** → 2단계에서 mono를 못 찾은 것. Cancel 누르고 CrossOver 설치 상태에서 `build-engine.sh` 재실행.
- **게임이 86MB/0% CPU로 영원히 멈춤** → `ROSETTA_ADVERTISE_AVX=1`이 게임에 전달되지 않은 것. 반드시 `play.sh`로 실행.
- **라이브러리 로드 실패(gnutls/freetype)** → `nohup`/`arch` 등 애플 서명 바이너리를 거쳐 실행하면 `DYLD_*`가 제거됨. `play.sh`로 실행.
- **배틀넷 로그인 화면이 가끔 깜빡임(~1분 1회)** → 알려진 외관 이슈, 자동 복구되며 로그인 정상 동작.
- **BLZBNTBNA00000005** → `play.sh`가 서명 exe를 자동 시드함. `play.sh`로 실행했는지 확인.

## 라이선스

- 이 레포의 스크립트·문서: **GPL-3.0**
- 엔진은 CodeWeavers 공개 Wine 소스(GPL/LGPL)로 빌드 — 소스: `media.codeweavers.com/pub/crossover/source/`
- 미포함(앞으로도): 애플 D3DMetal/GPTK(재배포 금지), CrossOver 앱 바이너리, 블리자드 파일 일체
- 본 프로젝트는 CodeWeavers·Apple·Blizzard와 무관합니다. Battle.net과 Diablo는 Blizzard Entertainment의 상표입니다.
