# 벽 3 — D2R 게임 본체 구동 (진행 중)

로그인 + Agent + "playable" 인식까지 무료 스택으로 성공(벽1·2 해결). 그러나 **게임 본체 D2R.exe 실행**에서 막힘.

## 증상
- Battle.net이 D2R.exe를 실행(`Launched .../D2R.exe -uid osi`, gameRunning=1) 후 ~6~9초 만에 종료, 고아 프로세스만 0% CPU로 잔존.
- D2R이 `D2R_loader.dll`까지 로드 후 **그래픽 이전 단계에서 정지**(WINHTTP·bcrypt 로드 후 무한 대기). 크래시 덤프/예외 없음.
- 오프라인 직접 실행(`D2R.exe`, Battle.net 없이)도 동일하게 정지.

## 근본 원인 (외부 요인, 우리 셋업 아님)
2026-01 Blizzard이 D2R에 새 안티치트를 추가하며 **Rosetta 2 버그**를 유발. 이 때문에 **유료 CrossOver에서도 D2R이 안 뜨던 시기가 있었음**(CodeWeavers 공식 인정, Apple/Blizzard 수정 대기). 수정은 **macOS 26.4+ Rosetta 업데이트**로 제공.

## 이 맥에서의 검증 (2026-08-26, macOS 26.5)
- **진짜 CrossOver 26.3으로 D2R 실행 → 성공.** `[D3DMetal:LOG]` 렌더링, CPU 100%+, 게임 구동 확인. → macOS 26.5의 Rosetta 수정 + CrossOver 26.3 wine32on64가 조합되면 D2R 구동됨.
- **무료 WineCX24(=CrossOver 24 커뮤니티 빌드)로는 실패** — 로더에서 정지. wine 버전이 너무 낮아 새 안티치트/Rosetta 경로 비호환.
- frankea/Whisky v4.5(=CX26.3 changes를 wine-11.15 new-wow64로 리베이스)는 **로그인 CEF에서 크래시**(new-wow64라 벽1 재발). → D2R은 될지 몰라도 로그인이 깨짐.

## 결론
D2R 완주에 필요한 것 = **CrossOver 26급 wine32on64**(구식 32-on-64 + 최신 로더). 현재 이걸 만족하는 무료 빌드가 없음:
- WineCX24: wine32on64 O, 최신 로더 X → 로더 정지
- frankea v4.5: 최신 로더 O, wine32on64 X(new-wow64) → 로그인 CEF 크래시

## 남은 선택지
1. **CrossOver 26.3 GPL 소스로 wine32on64 직접 빌드** — 소스는 공개(`media.codeweavers.com/pub/crossover/source/crossover-sources-26.3.0.tar.gz`, 142MB, wine에 win32on64 지원 확인). 단, CrossOver의 빌드 오케스트레이션은 비공개라 빌드 과정을 재구성해야 함(대형 작업, Gcenx/Sikarugir 수준). D3DMetal(Apple GPTK, 소스 미포함)은 설치된 CrossOver에서 이식 가능(그래픽은 이미 검증됨).
2. **커뮤니티 CX26 wine32on64 빌드 대기** — @dappermint가 CX26.3을 이미 리베이스 중. wine32on64 형태로 나오면 이 레포 스크립트로 즉시 적용, 벽1·2 해결분과 합쳐 완주 가능성. (가장 저비용)
3. CrossOver 구매.

## 이미 확보한 자산 (완주에 재사용)
- 벽1 해결(wine32on64 엔진), 벽2 해결(버전폴더 exe seed), 검증 보틀, D3DMetal 이식법(그래픽 검증됨).
- 엔진만 CX26급으로 교체되면 벽1·2·3 모두 해결되어 완주 예상.

## 업데이트 (2026-08-27): CrossOver 26.3 wine 직접 빌드 완료 + D2R 로더 벽

**성과**: CrossOver 26.3 GPL 소스(Wine 11.0)를 Apple Silicon에서 x86_64로 직접 빌드 성공(`scripts/build-engine.sh`). 무료·합법.
- Battle.net **로그인 + Agent + osi playable 전부 동작** (렌더러 죽음 0, TLS/폰트 정상).
- 즉 "무료로 Battle.net 구동"은 이 자체 빌드 엔진으로 달성.

**남은 벽**: D2R.exe가 `D2R_loader.dll`(Blizzard 안티치트/DRM) 로드 직후 **86MB/0%CPU로 정지** (main thread `mach_msg` 대기, 전 스레드 idle=외부 응답 대기). 같은 Wine 11.0 소스인데 **설치된 CrossOver 26.3 바이너리로는 D2R 구동됨**(이 맥에서 확인), 우리 빌드로는 정지.

**정지에 무관했던 시도(전부 86MB 동일)**: wine-mono 10.4.1 이식, CrossOver entitlement 서명, CrossOver 보틀 사용, DXMT(winemetal) 이식, msync/esync/fsync off, 배틀넷 세션 살린 상태의 -uid osi 실행.

**해석**: 차이는 CrossOver 바이너리에만 있는 부분 — 공개 GPL 소스에 없는 **게임별 프로프라이어터리 처리(CW HACK/DXMT 완본)** 또는 Rosetta2-안티치트 상호작용의 빌드/서명 세부. CodeWeavers도 이 안티치트+Rosetta 문제로 수개월 고생(Apple 26.4 Rosetta 수정 필요)했던 바로 그 지점.

**다음 후보**: (a) 우리 빌드 D2R vs CrossOver D2R를 동일 시점 sample 비교해 분기점 격리, (b) DXMT 완전본(d3d12 경로 포함) 확보, (c) 커뮤니티 CX26 wine32on64 빌드(@dappermint) 대기. 엔진은 재사용 준비 완료.

## ✅ 최종 해결 (2026-08-27)

위 "해석"은 틀렸다 — CrossOver 바이너리만의 비밀이 아니라 **실행 환경 변수**였다.

**`ROSETTA_ADVERTISE_AVX=1`** — CrossOver의 `bin/wine` perl 래퍼가 조용히 설정하는 값. D2R 로더는 AVX 명령어 지원을 요구하며, Rosetta 2는 이 변수가 있어야 AVX를 CPUID에 광고한다. 이것 없이는 로더가 86MB/0%CPU로 영원히 대기한다(위에서 관찰한 그 정지).

이후 그래픽은 D3DMetal 심링크 레이아웃(lib/external 실물 + x86_64-unix 심링크, 복사 금지)과 GPTK PE dll(d3d11/d3d12/dxgi) + `CX_ACTIVE_GRAPHICS_BACKEND=d3dmetal`로 해결. 자체 빌드 wine 11.0(신형 WoW64)에서 **D2R 인게임 렌더링 확인** (메모리 2.7GB, CPU 175%, D3DMetal 로그).

전체 실행 조합은 `scripts/play.sh` 참고. wine32on64가 필요하다던 중간 결론도 폐기 — 신형 WoW64로도 CEF·게임 모두 동작한다(벽1의 진짜 원인은 엔진 계열이 아니라 조합 문제였음).

## 추가 발견 (2026-08-27): libd3dshared는 그래픽이 아니라 '로더 통과'에 필요하다

순수 vkd3d(D3D12→Vulkan→MoltenVK) 경로 실험 결과:

- `ROSETTA_ADVERTISE_AVX=1`만으로는 부족 — libd3dshared 없이는 여전히 86MB 정지.
- `CX_APPLEGPTK_LIBD3DSHARED_PATH`를 지정하면(ntdll의 `init_non_native_support`가 dlopen하여 `register_non_native_code_region`을 확보) **D3DMetal 그래픽 없이 vkd3d만으로도 게임이 완주**한다 (2.7GB/CPU 600%+, MoltenVK 렌더링).

**결론: D2R 로더(안티치트)의 Rosetta 통과 조건 = AVX 광고 + GPTK의 비네이티브 코드영역 등록, 두 가지 모두.** libd3dshared는 이 후자를 제공하는 유일한 공급원이므로, GPTK(또는 CrossOver)에서 이 파일 하나는 반드시 조달해야 한다. 그래픽 백엔드는 vkd3d(완전 오픈소스)로 대체 가능.
