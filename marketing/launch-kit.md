# Soju launch kit

Drafts for manual publication. Positioning: a free, open-source game launcher toolkit for Apple Silicon Macs.

Soju sets up Battle.net, Steam, Epic Games Launcher and GOG GALAXY. It provides terminal setup, separate Wine environments and a Mac launch app for each store. The store clients retain their own interfaces; a unified library UI is not currently provided.

## English community post

Title: Soju: an open-source toolkit for Windows game launchers on Apple Silicon

I'm building Soju, a free, open-source toolkit that sets up Battle.net, Windows Steam, Epic Games Launcher and GOG GALAXY on Apple Silicon Macs.

Choose the launchers you use, install them in separate Wine environments, and open them through Mac launch shortcuts. You sign in with your existing store accounts.

Current verified examples on M4 Pro / macOS 26.5 include Hogwarts Legacy through Epic, Diablo II: Resurrected through Battle.net, and a Unity D3D11 title through Steam with a custom DXMT setup. GOG login and library access are verified; its games have not been broadly tested.

Steam game rendering needs additional setup. D2R needs macOS 26.4 or later, and Apple's GPTK components for the Battle.net/Epic/GOG path are downloaded separately. Launcher support does not imply that every game works.

I'm looking for compatibility reports across different games and Macs: game + store, Mac model, macOS version, Soju/engine version, and what worked or failed. Reproducible bug reports and contributions are welcome.

Project and installation: https://soju.snack-wrap.com/
Source: https://github.com/BCD1210/soju
Reports: https://github.com/BCD1210/soju/discussions

## Short post

Soju is a free, open-source launcher toolkit for Apple Silicon Macs: Battle.net, Steam, Epic and GOG, with separate Wine environments and Mac launch shortcuts. Compatibility varies by game. Try it and share what works: https://soju.snack-wrap.com/

## 한국어 커뮤니티 글

제목: Apple Silicon 맥용 오픈소스 게임 런처 도구 Soju를 만들고 있습니다

Soju는 맥에서 Battle.net, Windows Steam, Epic Games Launcher, GOG GALAXY를 설치하고 실행하도록 돕는 무료 오픈소스 프로젝트입니다.

터미널에서 사용할 런처를 선택하면 각각 분리된 Wine 환경에 설치하고, 맥에서 실행할 수 있는 바로가기를 만듭니다. 로그인과 라이브러리는 각 스토어의 기존 화면을 사용합니다.

현재 M4 Pro / macOS 26.5에서 Epic의 호그와트 레거시, Battle.net의 디아블로 2: 레저렉션, 별도 DXMT 설정을 사용한 Steam의 Unity D3D11 게임 실행을 확인했습니다. GOG는 로그인과 라이브러리까지 확인했으며 게임 호환성은 추가 검증이 필요합니다.

Steam 게임 실행에는 추가 설정이 필요하고, 디아2는 macOS 26.4 이상이 필요합니다. Battle.net·Epic·GOG 경로에서 사용하는 Apple GPTK 구성요소는 별도로 내려받습니다. 모든 게임의 실행을 보장하는 단계는 아닙니다.

다양한 게임과 맥에서 결과를 모으고 있습니다. 게임명·스토어·맥 모델·macOS 버전·Soju/엔진 버전과 함께 성공 사례나 재현 가능한 오류를 알려주시면 도움이 됩니다.

소개와 설치: https://soju.snack-wrap.com/
소스: https://github.com/BCD1210/soju
실행 결과: https://github.com/BCD1210/soju/discussions

## Demo recording plan

Capture the real installed software; this is a storyboard, not a finished recording.

- 0–5 seconds: title, “Your Windows launchers. On your Mac.”
- 5–15 seconds: show the launcher choices and installed Mac shortcuts.
- 15–30 seconds: show available launcher windows and libraries, with private account details hidden.
- 30–40 seconds: show actual gameplay from verified examples and label game, store, Mac and macOS version.
- 40–45 seconds: project URL and request for compatibility reports.

Include Steam's extra DXMT setup and GOG's limited verification in the accompanying text. Never label a launcher library screen as verified gameplay.

## Publication notes

- Check the destination's current self-promotion rules before posting.
- Use an existing project thread for a substantive update when appropriate; avoid duplicate launch posts.
- Keep D2R-specific guides discoverable while the main project introduction covers all four launchers.
- Link to compatibility notes and ask for evidence, not only stars.
- Record the post URL and date after publishing. These drafts have not been submitted to communities.
