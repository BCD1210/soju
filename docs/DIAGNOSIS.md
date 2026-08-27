# 기술 진단

macOS(Apple Silicon)에서 무료 Wine으로 Battle.net을 돌릴 때의 두 개의 벽과 그 원인.

## 벽 1 — CEF 로그인 웹뷰 렌더러 크래시 (해결됨)

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
- GL 에러(`SharedImageStub`, `Failed to create ... context for virtualization`)는 **무해** — 진짜 CrossOver도 동일하게 99개씩 냄.

## 벽 2 — Battle.net Agent caller 서명 검증 실패 (해결됨)

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
