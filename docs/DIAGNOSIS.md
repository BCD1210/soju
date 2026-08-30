# 기술 진단

macOS(Apple Silicon)에서 무료 Wine으로 Battle.net을 돌릴 때의 두 개의 벽과 그 원인.

## 벽 1: CEF 로그인 웹뷰 렌더러 크래시 (해결됨)

### 증상
로그인 창이 검은 화면 + 스피너만. 렌더러가 5초마다 재시작:
```
[Browser] Render process was terminated and reloaded automatically.
wine: Unhandled exception 0x80000003 in thread XXX at address 6DDB00E1
```
크래시 스레드 백트레이스: 시작 직후 `0x000000fff40d6c`(잘못된 주소)로 점프 → int3. 프레임은 libcef 내부.

### 원인
stock Whisky / frankea 엔진(v3.1.1, v4.5beta, v2.5.0)은 전부 **신형 WoW64**(64비트 ntdll이 32비트를 thunk). Battle.net은 32비트 앱이고, 그 안의 CEF(Chromium)가 스레드를 생성하는 방식이 신형 WoW64 계층에서 깨진다. `bin/`에 `wine64`만 있고 `wine32on64`가 없는 것이 지표.

### 해결
**wine32on64**(구식 32-on-64, CrossOver 방식) 엔진 사용:
- **WineCX 24.0.7** (KegworksCX, CrossOver 24 GPL 소스 커뮤니티 빌드)
- 출처: `Sikarugir-App/Engines` 릴리스 → `WS12WineCX24.0.7.tar.xz`
- 단일 `wine` 바이너리가 32on64 (Intel → Rosetta 2)
- 결과: 렌더러 죽음 0, 로그인 웹앱 로드, **실제 로그인 성공**

부수 조건:
- 번들에 dylib이 없음 → Whisky 엔진의 `Wine/lib/*.dylib`를 번들 `bin/`에 복사 + `install_name_tool -add_rpath @loader_path/` + adhoc 재서명. MoltenVK는 `lib/wine/x86_64-unix/`에도 복사.
- 유저 `AppData/Roaming/Battle.net`(Battle.net.config) 존재해야 로그인 URL 로드됨.
- `--disable-gpu-compositing`로 실행 (소프트웨어 합성).
- GL 에러(`SharedImageStub`, `Failed to create ... context for virtualization`)는 **무해**. 진짜 CrossOver도 동일하게 99개씩 냄.

## 벽 1, 진짜 원인 확정: `WINE_SIMULATE_WRITECOPY` (2026-08-29)

위 "신형 WoW64" 설명은 결과적으로 맞는 엔진을 고르게 했지만 원인은 아니었다. CrossOver 26.3 GPL 소스로
직접 빌드한 엔진(`cx26-engine`)에서도 같은 int3(`libcef.dll+0x16D00E1`)가 재현됐고, CX 출하 바이너리의
`ntdll.so` 하나만 바꿔 끼우면 사라졌다. 동적 추적(`WINEDEBUG=+seh,+virtual,+process,+loaddll`)으로 특정한 내용:

- 크래시 프로세스: `Battle.net.exe --type=renderer` (CEF 렌더러)의 초기 스레드, DLL 로드 직후.
- 크래시 지점 디스어셈블(libcef RVA `0x16D00A0`):
  ```
  ret = VirtualProtect(addr, size, PAGE_READONLY /*2*/, &old);
  if (!ret)   int3;          // 0x16D00DE
  if (old != PAGE_READWRITE /*4*/) int3;   // 0x16D00E1  ← 여기서 죽음
  ```
- `addr`는 `libcef.dll`의 `.data` 페이지(이미지 매핑, write-copy). 순정 wine은 이미지의 쓰기 가능 페이지를
  처음부터 RW로 mmap 하고 첫 쓰기를 추적하지 않으므로 `old`가 영원히 `PAGE_WRITECOPY(8)`이다.
  Windows는 첫 쓰기 후 `PAGE_READWRITE(4)`로 바뀐다.
- CX 소스에는 이를 흉내 내는 **CW Hack 22996** (`simulate_writecopy`, `dlls/ntdll/unix/virtual.c`
  `NtProtectVirtualMemory`의 "Setting VPROT_COPIED")가 있지만, 공개 소스는 `WINE_SIMULATE_WRITECOPY` 환경변수로만
  켜진다(`loader.c: hacks_init`). CX 출하 바이너리는 같은 환경에서 이 경로를 탄다 → 기본값이 다름.

검증 (각 75~90초, 같은 보틀):

| 엔진 | `EXCEPTION_BREAKPOINT` | 렌더러 생존 |
|---|---|---|
| cx26-engine (우리 빌드) | 2 | ✗ |
| cx26-engine + SDK15 재빌드 ntdll | 2 | ✗ (툴체인 가설 기각) |
| cx26-engine + CX ntdll.so | 0 | ✓ |
| cx26-engine + `WINE_SIMULATE_WRITECOPY=1` | 0 | ✓ (90초 후 렌더러 2개 생존, 로그인·메인창 OK) |

해결: 모든 실행 경로(`install.sh`, `scripts/play.sh`, `scripts/create-bottle.sh`)에 `WINE_SIMULATE_WRITECOPY=1`.
CrossOver 바이너리 의존 0.

## 벽 1-b, CX 의존 #2: 투명한 메인창 (`--in-process-gpu --use-gl=swiftshader`) (2026-08-29)

`WINE_SIMULATE_WRITECOPY=1`만으로는 렌더러는 살지만 **Dock 아이콘만 뜨고 창이 안 보인다**. `CGWindowListCopyWindowInfo`로
보면 1600×1000 창이 화면 정중앙에 "존재"(alpha 1)하지만 내용이 없다. Battle.net 메인창은 frameless라 내용이 안 그려지면
완전히 투명하다. libcef 로그 비교:

- CX ntdll: GPU 프로세스 없음(NtCreateUserProcess `--type=gpu-process` 0건), 브라우저 프로세스 안에서 GLES 컨텍스트 생성.
- 우리 ntdll: `--type=gpu-process`가 생성되고 `Exiting GPU process due to errors during initialization` ×9 →
  `GL is disabled` → 아무것도 합성되지 않음.

원인: A에서 Battle.net.exe가 받는 인자가 `--disable-gpu-compositing --from-launcher --in-process-gpu --use-gl=swiftshader`.
`Battle.net Launcher.exe`는 양쪽 다 앞의 두 개만 넘기고(`+relay`로 확인), 뒤의 두 개는 **CX ntdll.so의 NtCreateUserProcess
내부에서 주입**된다. 출처는 CodeWeavers의 비공개·암호화 호환 DB(`~/Library/Application Support/CrossOver/compatdb-26.dat`,
`cxcompatdb.so`)의 앱별 규칙. GPL 소스에도, 바이너리 문자열에도 없다.

해결: Launcher.exe를 거치지 않고 `Battle.net.exe --disable-gpu-compositing --from-launcher --in-process-gpu --use-gl=swiftshader`를
직접 실행 (`install.sh` 런처, `scripts/play.sh`). 검증: GPU 프로세스 0, GLES 컨텍스트 생성, 렌더러 6, 로그인·메인창, **화면 표시·게임 실행 사용자 확인**.

## Epic Games Launcher: 같은 엔진에서 추가 조치 없이 동작 (2026-08-29)

벽 1/1-b가 배틀넷 고유 문제인지 CEF 일반 문제인지 확인하려고 Epic Games Launcher(UE5 + CEF3 `EpicWebHelper.exe`, Chrome/90)를 같은 엔진·같은 env(`WINE_SIMULATE_WRITECOPY=1` 포함)로 돌렸다.

- 설치: 공식 `EpicGamesLauncherInstaller.msi`를 `msiexec /i … /qn`으로 무인 설치, 26초. 메뉴빌더 에러(`cx_wineshelllink`)만 나고 무해.
- 실행: `EpicGamesLauncher.exe` 기본 인자 그대로. CEF가 `--type=gpu-process`를 별도 프로세스로 띄우는데 **죽지 않는다**. 배틀넷과 달리 `--in-process-gpu --use-gl=swiftshader`가 필요 없었다. 1826×857 메인창, 로그인, 스토어 UI 렌더링 확인.
- wined3d는 `Using the Vulkan renderer for d3d10/11`(MoltenVK) 경로를 탔다. 런처 UI 자체는 이걸로 충분.
- 로그의 `LogDPoP: Failed to create persistent DPoP key (0x80090029)`는 Linux Wine에서도 나오는 것으로 로그인에 영향 없음.
- 트레이 UX: 창 닫기 = 숨김(Windows와 동일). Epic이 `Shell_NotifyIcon`으로 등록한 트레이 아이콘을 winemac systray가 **macOS 메뉴바 NSStatusItem**으로 올리고(`+systray` 트레이스로 확인), **우클릭** 메뉴로 창 열기·Exit가 된다, 사용자 검증 완료. 좌클릭은 WM_LBUTTONDOWN/UP+NIN_SELECT까지 정상 전달되지만 Epic이 반응하지 않는다(Windows에서도 메뉴 기반). 시도했다 버린 것들: exe 재실행·`com.epicgames.launcher://` URL은 실행 중 인스턴스가 무시; 외부에서 `ShowWindow(SW_SHOW/RESTORE)`로 숨긴 창을 띄우면 보이긴 하지만 Slate가 "최소화" 상태를 유지해 **입력을 안 받는 먹통 창**이 된다. 그래서 Epic 모드는 `WINE_DOCK_REOPEN_CMD`를 쓰지 않는다.
- 부수 수정: `WINE_DOCK_REOPEN_CMD` 훅의 "보이는 창" 판정이 Epic의 화면 밖(-10000,-10000) 1×1 투명 보조창을 보이는 창으로 세던 것을 실제 화면 안·alpha>0·크기>1 조건으로 고쳤고, `build-engine.sh`가 CX 엔진에도 winemac 패치를 적용한다.

결론: 벽 1(`WRITECOPY`)은 libcef 버전에 따라 걸리는 일반 문제일 수 있지만, 벽 1-b(GPU 프로세스 사망)는 Battle.net의 CEF 빌드/설정 고유. 다른 CEF 런처(GOG Galaxy·EA app·Ubisoft Connect)도 같은 절차로 먼저 "그냥 돌려보는" 것이 맞다.

## 벽 2: Battle.net Agent caller 서명 검증 실패 (해결됨)

### 증상
로그인 성공 후: `Oops! An error occurred while loading game information ... BLZBNTBNA00000005`.
Battle.net 클라 로그:
```
[Agent] AgentClient failed to connect, CURL error=7
[Agent] Agent restart limit exceeded
```
Agent 로그:
```
Unable to validate connecting process (268)
Failed Caller authorization due to signature for 'Battle.net.exe'(268)
```

### 근본 원인 (핵심)
Agent는 자기한테 접속한 프로세스(Battle.net.exe 클라)가 정품 서명됐는지 `WinVerifyTrust`로 검사한다. wintrust 트레이스:
```
wintrust:dump_file_info pcwszFilePath: L"Battle.net.exe"      ← 파일명만!
wintrust:SOFTPUB_OpenFile returning 2                          ← 2 = FILE_NOT_FOUND
wintrust:WINTRUST_DefaultVerify returning 00000002             ← 검증 실패
```
반면 정상 검증(Agent.exe 자기 자신)은 **풀 경로**:
```
wintrust:dump_file_info pcwszFilePath: L"C:/ProgramData/Battle.net/Agent/Agent.exe"
```

**결론**: Agent가 접속 프로세스의 PID(268)로 이미지 경로를 조회했는데, wine이 **전체 경로 대신 basename(`Battle.net.exe`)만** 반환한다. wintrust는 상대 파일명을 CWD에서 못 찾아 FILE_NOT_FOUND → 서명 검증 실패 → caller 거부.

- iphlpapi `GetExtendedTcpTable`(class 5 = TCP_TABLE_OWNER_PID_ALL)는 호출되고 PID(268)는 나옴 → PID 조회 자체는 됨.
- 문제는 그 다음 **PID → 전체 이미지 경로** 변환.

### 정확한 메커니즘 (추적 결과)
- Agent는 `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)`로 프로세스를 열거해 접속 PID의 이름을 얻는다 → `PROCESSENTRY32.szExeFile` = basename `"Battle.net.exe"` (이건 Windows에서도 basename이 정상).
- 이 basename을 `WinVerifyTrust`(WINTRUST_FILE_INFO.pcwszFilePath)에 그대로 넘긴다.
- wine `dlls/wintrust/softpub.c`의 `SOFTPUB_OpenFile`이 `CreateFileW("Battle.net.exe", ...)` 호출 → 상대 경로라 **프로세스의 윈도우 CWD** 기준으로 해석.
- Agent의 CWD(unix `lsof`로 확인) = `.../Program Files (x86)/Battle.net/Battle.net.NNNNN` (버전 하위폴더). 그런데 서명된 `Battle.net.exe`는 그 **부모** `.../Battle.net/`에 있음 → CWD엔 없음 → `CreateFileW` = ERROR_FILE_NOT_FOUND(2) → `WINTRUST_DefaultVerify returning 2` → caller 거부.
- (Windows에선 이 코드가 통과 = Agent의 CWD가 Battle.net 루트이거나, 어쨌든 그 폴더에서 exe가 찾아짐. 즉 이건 wine의 CWD/파일해석 차이지 서명 자체 문제가 아님.)

### 해결책 (채택)
각 버전 하위폴더(`Battle.net.NNNNN`)에 **서명된 `Battle.net.exe` 사본**을 넣는다. 그러면 Agent의 CWD에서 상대 파일명이 유효한 서명 파일로 풀려 검증 통과.
- `scripts/launch.sh`가 매 실행 시 루트의 `Battle.net.exe`를 모든 `Battle.net.[0-9]*` 하위폴더에 동기화(APFS 클론). 업데이트로 버전 번호가 바뀌어도 자동 대응.
- **wine 재빌드/패치 불필요.** 결과: sig 실패 0, `InstallState (osi): playable=1`, `GameController: Selecting game family by id: OSI`(D2R).

### (대안) 근본 wine 패치
더 깔끔하게는 wine이 프로세스의 윈도우 CWD를 unix cwd와 동기화하거나, `SOFTPUB_OpenFile`이 실패 시 이미지 디렉토리를 탐색하게 패치할 수도 있으나, 파일 seed 방식이 재빌드 없이 견고하므로 채택하지 않음. 리눅스가 문제없는 것도 CWD 해석 차이 때문으로 추정.
