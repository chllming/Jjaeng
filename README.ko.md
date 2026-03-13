# Jjaeng

[English](README.md) | 한국어

Jjaeng은 Wayland와 Hyprland 환경을 위한 스크린샷 및 녹화 도구입니다. 백그라운드 데몬, 좌하단 컴팩트 미리보기 흐름, 스크린샷/영상 히스토리, Omarchy 기반의 각진 평면 UI, 내장 주석 편집기를 제공합니다.

Jjaeng은 [ChalKak](https://github.com/BitYoungjae/ChalKak)에서 출발한 파생 프로젝트입니다. 업스트림 라이선스 모델은 그대로 유지하며, 출처 정보는 [NOTICE](NOTICE)에 정리되어 있습니다.

## 주요 기능

- 전체 화면, 영역, 창 캡처
- 전체 화면, 영역, 창 녹화와 크기/인코딩/오디오를 조정하는 녹화 프롬프트
- 데몬(`jjaengd`) 기반의 백그라운드 실행과 소켓 제어
- `Save` / `Copy` 중심의 컴팩트 미리보기와 `더블클릭` / `E`로 편집기 진입
- 썸네일, 빠른 저장/복사, 편집 진입이 가능한 이미지/영상 히스토리 화면
- 블러, 펜, 화살표, 사각형, 크롭, 텍스트, OCR 편집 도구
- Omarchy가 있으면 현재 팔레트와 메뉴 스타일을 읽어 미리보기, 히스토리, 런치패드, 녹화 프롬프트에 같은 평면 스타일 적용
- 클립보드 복사는 PNG 이미지로 제공
- 편집기 저장 형식 선택: PNG, JPEG, WEBP

## 워크스페이스 구성

- `crates/jjaeng-core`: 캡처, 저장소, 클립보드, OCR, IPC, 히스토리, 공용 서비스
- `crates/jjaeng-ui`: 미리보기, 히스토리, 런치패드, 편집기용 GTK 런타임
- `crates/jjaeng-daemon`: 숨김 데몬 바이너리 `jjaengd`
- `crates/jjaeng-cli`: 사용자용 CLI 바이너리 `jjaeng`

## 실행 요구사항

- Wayland
- Hyprland
- `grim`
- `slurp`
- `wl-clipboard`
- GTK4 런타임 라이브러리

## 설치

### AUR

```bash
yay -S jjaeng
```

사전 빌드 바이너리 패키지:

```bash
yay -S jjaeng-bin
```

OCR 모델:

```bash
yay -S jjaeng-ocr-models
```

### 소스 빌드

```bash
git clone https://github.com/chllming/Jjaeng.git
cd Jjaeng
cargo build --release --workspace
install -Dm755 target/release/jjaeng ~/.local/bin/jjaeng
install -Dm755 target/release/jjaengd ~/.local/bin/jjaengd
```

## 사용 예시

데몬 시작:

```bash
jjaengd
```

캡처:

```bash
jjaeng --capture-region
jjaeng --capture-window
jjaeng --capture-full
```

녹화:

```bash
jjaeng --record-region
jjaeng --record-region-prompt
jjaeng --record-window-prompt
jjaeng --stop-recording
```

히스토리와 후속 동작:

```bash
jjaeng --launchpad
jjaeng --toggle-history
jjaeng --open-history
jjaeng --open-preview
jjaeng --edit-latest
jjaeng --copy-latest
jjaeng --save-latest
jjaeng --status-json
```

## 데스크톱 연동

- Waybar 상태 스크립트: [scripts/jjaeng-waybar-status.sh](scripts/jjaeng-waybar-status.sh)
- Omarchy/Hyprland 연동은 `~/.config` 오버라이드에서 설정하는 것을 전제로 합니다
- Omarchy가 설치되어 있으면 Jjaeng은 현재 Omarchy 팔레트와 메뉴 타이포그래피를 런타임 기본 테마로 읽습니다

## 설정

설정 디렉터리:

- `$XDG_CONFIG_HOME/jjaeng/`
- fallback: `$HOME/.config/jjaeng/`

주요 파일:

- `config.json`
- `theme.json`
- `keybindings.json`

주요 설정:

- `screenshot_dir`: 기본 저장 폴더를 덮어씀 (기본값: `$HOME/Pictures`)

## 개발

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

## 라이선스

MIT 또는 Apache-2.0 이중 라이선스입니다. 자세한 내용은 [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE), [NOTICE](NOTICE)를 참고하세요.
