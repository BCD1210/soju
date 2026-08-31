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
| cx26-engine (우리 빌드) | 2 | X |
| cx26-engine + SDK15 재빌드 ntdll | 2 | X (툴체인 가설 기각) |
| cx26-engine + CX ntdll.so | 0 | O |
| cx26-engine + `WINE_SIMULATE_WRITECOPY=1` | 0 | O (90초 후 렌더러 2개 생존, 로그인·메인창 OK) |

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
- 트레이 UX: 창 닫기 = 숨김(Windows와 동일). Epic이 `Shell_NotifyIcon`으로 등록한 트레이 아이콘을 winemac systray가 **macOS 메뉴바 NSStatusItem**으로 올리고(`+systray` 트레이스로 확인), **우클릭** 메뉴로 창 열기·Exit가 된다, 사용자 검증 완료. 좌클릭은 WM_LBUTTONDOWN/UP+NIN_SELECT까지 정상 전달되지만 Epic이 반응하지 않는다(Windows에서도 메뉴 기반). 시도했다 버린 것들: exe 재실행·`com.epicgames.launcher://` URL은 실행 중 인스턴스가 무시; 외부에서 `ShowWindow(SW_SHOW/RESTORE)`로 숨긴 창을 띄우면 보이긴 하지만 Slate가 "최소화" 상태를 유지해 **입력을 안 받는 먹통 창**이 된다. `WM_SYSCOMMAND/SC_RESTORE`도 마찬가지로 먹통 창을 만든다. 유일하게 정상인 경로는 Epic 자신의 트레이 코드라서, 독 클릭은 트레이 더블클릭을 그대로 재현한다(`tools/soju-epic-restore.c`): Epic은 트레이 아이콘을 `uCallbackMessage=0x8054`(WM_APP+0x54), 버전 0으로 등록하므로(winemac `+systray` 트레이스에 콜백 메시지 출력을 추가해 확인) 런처 프로세스의 창들에 그 메시지를 wParam=1, lParam=`WM_LBUTTONDOWN/UP, NIN_SELECT, WM_LBUTTONDBLCLK, WM_LBUTTONUP, NIN_SELECT` 순으로 보내면 Slate가 스스로 창을 복원하고 입력도 정상이다. 단일 클릭(NIN_SELECT만)은 Epic이 무시한다.
- 부수 수정: `WINE_DOCK_REOPEN_CMD` 훅의 "보이는 창" 판정이 Epic의 화면 밖(-10000,-10000) 1×1 투명 보조창을 보이는 창으로 세던 것을 실제 화면 안·alpha>0·크기>1 조건으로 고쳤고, `build-engine.sh`가 CX 엔진에도 winemac 패치를 적용한다.

결론: 벽 1(`WRITECOPY`)은 libcef 버전에 따라 걸리는 일반 문제일 수 있지만, 벽 1-b(GPU 프로세스 사망)는 Battle.net의 CEF 빌드/설정 고유. 다른 CEF 런처(GOG Galaxy·EA app·Ubisoft Connect)도 같은 절차로 먼저 "그냥 돌려보는" 것이 맞다.

## Epic 게임 설치 실패 DP-05 / DP-06: wineserver에 inotify가 빠져 있었다 (2026-08-30)

증상: 런처 로그인·스토어까지 되는데 Install을 누르면 아무 변화가 없고, 런처 로그에 `DirectoryPreparation: Returning result: DP-05` 후 60초 뒤 `Reporting Alert code: DP-06`이 찍힌다.

원인: Epic 런처는 설치 폴더 준비를 별도 프로세스(`EpicGamesLauncher.exe -commandlet=prepareinstalldir`)에 맡기고, 둘은 `C:/ProgramData/Epic/EpicGamesLauncher/com/` 폴더에 파일을 쓰고 `ReadDirectoryChangesW`로 서로의 응답을 기다린다. Wine은 디렉터리 변경 알림을 inotify로만 구현하고(`server/change.c`), macOS에서는 CrossOver가 kqueue 기반 `libinotify`를 번들해 wineserver에 링크한다. 우리 빌드는 configure가 `libinotify`를 못 찾아(`config.log: Package libinotify was not found`) 알림 없이 빌드됐고, 양쪽이 서로의 파일을 영영 못 본 채 타임아웃(commandlet 50초 → DP-05, 런처 60초 → DP-06)으로 끝났다. `LogDirectoryWatcher: A directory notification for '.../com' was aborted`가 그 흔적이다.

확인: `otool -L cx26-engine/bin/wineserver`에 libinotify 없음. 진짜 CrossOver의 wineserver는 `@rpath/libinotify.0.dylib`를 링크한다.

해결: `third_party/libinotify-kqueue/sys/inotify.h`(MIT)를 동봉하고 configure에 `INOTIFY_CFLAGS/INOTIFY_LIBS`를 넘겨 wineserver를 libinotify에 링크(`build-engine.sh`), 설치 후 install name을 `@rpath/libinotify.0.dylib`로 교체. 적용 즉시 71GB Hogwarts Legacy 설치가 진행됐고 게임도 실행됐다(AMD 드라이버 경고창은 OK로 넘어감, D3D12는 D3DMetal이 처리). Sikarugir#256의 DP-07은 다른 원인(`GetNamedSecurityInfoW` 합성 SD, CX HACK 27245)이며 이 핵은 우리 엔진에 이미 들어 있다.

부수 확인: 한글 IME가 켜져 있으면 게임이 키 입력을 못 받는다(입력기를 ABC로). 이 alert 코드들은 Whisky/Kegworks 계열 빌드에도 같은 이유로 해당될 가능성이 높다(그쪽 wineserver의 libinotify 링크 여부로 판별 가능).

## GOG GALAXY 검은 창: Qt D3D11 합성 + 덮어쓰이는 Chromium 플래그 (2026-08-30)

GOG GALAXY 2.1.8은 CEF가 아니라 **Qt6 + QtWebEngine**(Chromium 118/125)이다. Sikarugir#257·#258, Whisky#1004에 보고된 "흰/검은 창"을 이 엔진에서 재현하고 원인을 갈랐다.

1. D3DMetal에서는 QtWebEngine의 네이티브 합성기(`native_skia_output_device.cpp`)가 Chromium D3D11 텍스처를 Qt의 D3D11 RHI와 공유하려고 `QueryInterface(IDXGIResource)`를 부르는데 D3DMetal의 DXGI가 `E_NOINTERFACE(80004002)`를 돌려준다. 그 뒤 `CreateSharedImage failed` → `context is marked as lost`가 초당 수백 번 찍히고 창은 검다.
2. 보틀만 wined3d(마커 제거한 wine-stable 11.0 PE, `WINEDLLOVERRIDES=d3d11,dxgi,d3d10core,wined3d=n`)로 바꾸면 그 에러는 사라지지만, ANGLE이 wined3d의 FL 9_3에서 GLES 3.0을 못 만들고(`Renderer11.cpp:1107`, `too few uniforms`) 창은 투명(GL 렌더러: `glClear`에서 `GL_INVALID_FRAMEBUFFER_OPERATION`, Vulkan 렌더러: 에러 없이도 안 그림; 레이어드 창에 D3D 프레젠트가 안 되는 Wine 한계로 보임).
3. Qt RHI를 OpenGL로 바꾸면(`QSG_RHI_BACKEND=opengl`) Chromium이 WGL pbuffer를 못 만들어(`gl_surface_wgl.cc: Unable to create pbuffer`) int3, Vulkan으로 바꾸면 즉시 c0000409.
4. 남은 답은 Chromium을 CPU 합성으로 돌리는 것이다. 그런데 GOG는 `QTWEBENGINE_CHROMIUM_FLAGS`를 **자기가 설정**해서(바이너리에 변수명 존재) 밖에서 준 값을 덮어쓰고, 자기 argv의 Chromium 스위치는 무시하며(`--verbose-logging` 등 자기 옵션만 파싱), 실행 파일을 바꾸면 "executable checksum doesn't match"로 거부한다. 즉 사용자 공간에서는 스위치를 넣을 길이 없다.

해결(엔진 훅, `patches/chromium-flags-append.patch`): kernelbase `SetEnvironmentVariableW`와 msvcrt/ucrtbase `env_set`(CRT `_putenv_s` 경로, Qt의 `qputenv`가 쓰는 곳)에서 이름이 `QTWEBENGINE_CHROMIUM_FLAGS`이고 `SOJU_CHROMIUM_FLAGS`가 있으면 그 값을 뒤에 덧붙인다. CRT 테이블과 Win32 블록 둘 다 갱신되므로 Qt의 `qgetenv`(CRT `getenv`)가 덧붙은 값을 읽는다. CodeWeavers가 같은 파일에 Ubisoft Connect용으로 `VK_ICD_FILENAMES` 훅을 둔 것과 같은 종류다.

검증: `play.sh gog`가 `SOJU_CHROMIUM_FLAGS="--disable-gpu --disable-gpu-compositing"`를 설정하면 D3DMetal 그대로 에러 3개(`WSALookupServiceBegin`, GLES3 폴백 2개)만 남고 로그인 창·로그인·메인 창(1732×798)이 뜬다. 렌더러 프로세스 커맨드라인에 `--disable-gpu-compositing`이 보인다.

타이틀바: GOG 메인 창은 `WS_OVERLAPPEDWINDOW`이지만 `WM_NCCALCSIZE`로 클라이언트를 창 전체로 잡고 자기 타이틀바를 그린다. Mac 드라이버는 `WS_CAPTION`이면 Cocoa 타이틀바를 얹고(`get_window_features_for_style`) `GetWindowStyleMasks`로 win32u에 캡션을 자기가 그린다고 알리므로 UI 첫 줄이 타이틀바 밑에 숨는다. winemac 패치에 `WINE_CUSTOM_FRAME`(세미콜론 구분 exe 목록)을 추가해 해당 프로세스는 두 곳 모두에서 title_bar를 끈다(`play.sh gog`가 `GalaxyClient.exe`로 설정). Win32 창 크기와 Cocoa 창 크기가 일치하는 것으로 확인.

트레이 복귀: 창을 닫으면 GOG는 트레이로 들어간다. 두 번째 `GalaxyClient.exe` 인스턴스는 `FindWindowW("GalaxyClientClass")`로 메인 창을 찾아 `WM_COPYDATA`(dwData=1, 16바이트: 자기 이미지 안 `RestoreClientMessage` vtable 포인터 `0x140a74858` + dword 1)를 보내고 종료하는데, 수신 측이 그 포인터를 그대로 역참조한다(같은 exe라 주소가 같아 동작). `tools/soju-gog-restore.c`가 이 메시지를 직접 보내 독 아이콘 클릭 시 0.2초 안에 창이 돌아온다. vtable 주소는 GOG 버전마다 바뀌므로 도구가 실행 시 GalaxyClient.exe의 MSVC RTTI(맹글드 클래스명 → 타입 디스크립터 → complete object locator → vtable)를 파싱해 계산하고, 실행 중인 클라이언트의 실제 모듈 베이스를 더한다. 버전 상수 없음. 미끼 창(같은 클래스명)으로 페이로드를 캡처했다.

부수: 강제 종료 후엔 `ProgramData/GOG.com/Galaxy/lock-files/`의 잠금 파일 때문에 "Second client instance detected"로 즉시 종료되므로 `play.sh gog`가 실행 전에 지운다. 설치기가 자동 실행하는 `GalaxyClient.exe /installerLaunch /payload=`는 빈 payload로 `campaignParamsForLogIn` 설정 오류를 내고 죽는데 무해하다. 이 훅은 Qt/QtWebEngine 기반 런처 전반(다른 스토어 클라이언트 포함)에 그대로 쓸 수 있다.

## 트레이 복귀 정리: 네 런처 모두 독 아이콘 클릭으로 돌아온다 (2026-08-30)

winemac 패치의 `WINE_DOCK_REOPEN_CMD`(보이는 창이 없을 때 독 클릭 시 실행)에 런처별 복원 명령을 연결했다.

- Battle.net: 기본은 X = 종료(Windows와 동일). `Client.HideOnClose`로 트레이 최소화를 켠 경우 두 번째 `Battle.net.exe` 실행이 기존 인스턴스에 넘겨져 창이 뜬다.
- Steam: `steam.exe steam://open/main` 재실행.
- Epic: 트레이 더블클릭 시퀀스 재현(`tools/soju-epic-restore.c`, 위 참조).
- GOG: `RestoreClientMessage` 직접 전송(`tools/soju-gog-restore.c`, 위 참조).

## Epic 로그인 `too_many_sessions`: ncrypt에 영구 키가 없어서 매번 새 기기가 된다 (2026-08-31)

### 증상

Epic Games Launcher 로그인 화면에서 계속 거부된다.

```
Sorry, your account has too many active logins.
Please log out of one of the places you are using your account and try again.
```

화면 문구는 "다른 곳에서 로그아웃하라"지만, 서버가 실제로 돌려주는 것은 다른 이야기다.

```
code=400 errorcode=errors.com.epicgames.account.oauth.too_many_sessions
numericErrorCode=18048
"Sorry too many sessions have been issued for your account. Please try again later"
```

`issued`(발급됨)이지 `active`(사용 중)가 아니다. 보유 중인 세션 수 제한이 아니라 일정 시간 안에
발급된 토큰 수에 대한 레이트 리밋이다. 그래서 모든 기기에서 로그아웃해도, 비밀번호를 재설정해
기존 세션을 전부 무효화해도 풀리지 않는다. 오히려 재시도할 때마다 카운터가 더 찬다.

### 근본 원인

런처 로그 17개를 확인하니 **매 실행마다** 같은 줄이 먼저 나온다.

```
LogDPoP: Error: Failed to create persistent DPoP key (Status: 0x80090029)
LogDPoP: Warning: Cannot build DPoP proof: no public JWK available
LogOnlineIdentity: Warning: OSS: DPoP enabled but proof build failed - sending request without DPoP header
```

DPoP(RFC 9449)는 발급한 토큰을 그 기기의 서명 키에 묶는 방식이다. Epic은 이 키로 "같은 기기가
맞다"를 확인하고, 맞으면 저장된 세션을 갱신해서 재사용한다. 키를 못 만들면 매 실행이 처음 보는
기기가 되므로 갱신 대신 **새 세션 발급**이 일어난다. 열 번 실행하면 세션도 그만큼 새로 발급된다.

0x80090029는 `NTE_NOT_SUPPORTED`다. 엔진에서 직접 확인한 결과:

```
NCryptOpenStorageProvider     = 0x00000000
NCryptOpenKey (기존 키 열기)   = 0x80090029
NCryptCreatePersistedKey ES256 = 0x80090029
```

wine의 `dlls/ncrypt/main.c`를 보면 이유가 그대로 적혀 있다. `NCryptCreatePersistedKey`는 키 이름을
받으면 `FIXME("Persistent keys are not supported")`를 찍고 무시하며, `NCryptOpenKey`는 통째로
스텁이다. 게다가 이 버전은 RSA만 처리해서 DPoP가 쓰는 ECDSA P-256은 아예 거부한다. 즉 wine에서
CNG 키는 프로세스와 함께 사라지고, 같은 이름으로 다시 열 방법이 없다.

### 해결책 (`patches/ncrypt-persisted-keys.patch`)

이름이 붙은 키를 Windows가 쓰는 위치(`%APPDATA%\Microsoft\Crypto\Keys`)에 저장하고 다시 읽는다.

- `NCryptCreatePersistedKey`: 이름을 기억하고, ECDSA P-256/P-384를 RSA와 함께 처리한다
- `NCryptFinalizeKey`: 키가 확정된 뒤 개인키 blob을 파일로 내보낸다 (EC 키는 곡선이 길이를 정하므로
  `BCryptSetProperty(BCRYPT_KEY_LENGTH)`를 건너뛴다)
- `NCryptOpenKey`: 저장된 키를 불러온다. 저장된 적이 없으면 Windows와 같이 `NTE_BAD_KEYSET`을
  돌려줘서, 호출자가 포기하지 않고 새로 만들도록 한다
- `NCryptDeleteKey`: 저장 파일도 함께 지운다

### 검증

프로브를 두 번 실행:

```
1회차: NCryptOpenKey = 0x80090016 (아직 없음) -> Create/Finalize 성공 -> 공개키 72바이트, 서명 64바이트
2회차: NCryptOpenKey = 0x00000000 (저장된 키 로드) -> 공개키 바이트가 1회차와 동일
```

2회차의 공개키가 1회차와 같다는 것이 핵심이다. 새로 만든 것이 아니라 디스크에서 같은 키를 불러왔다는
뜻이고, DPoP가 요구하는 성질이 바로 이것이다.

회귀는 wine의 ncrypt 적합성 테스트로 확인했다. 원본과 패치본 모두 436개 실행, 176개 todo,
**0 failures**로 동일하다. 배틀넷도 패치본 엔진에서 정상 부팅한다(BREAKPOINT 0, ncrypt 오류 0).

아직 Epic 로그인으로 끝까지 확인하지는 못했다. 이미 걸린 레이트 리밋이 풀려야 시도할 수 있고,
재시도 자체가 리밋을 다시 채우기 때문이다.

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
