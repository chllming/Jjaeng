# Maintainer: Jjaeng contributors
pkgname=jjaeng
pkgver=0.6.0
pkgrel=2
pkgdesc="Hyprland screenshot, recording, and MCP utility"
arch=('x86_64' 'aarch64')
options=(!lto)
url="https://github.com/chllming/Jjaeng"
_srcname="Jjaeng"
license=('MIT' 'Apache-2.0')
depends=('gtk4' 'hyprland' 'grim' 'slurp' 'wl-clipboard')
makedepends=('rust' 'cargo' 'pkgconf' 'gtk4' 'cmake' 'clang' 'git')
optdepends=('gpu-screen-recorder: GPU-accelerated Wayland recording and combined audio'
            'ffmpeg: recording thumbnail extraction'
            'jjaeng-ocr-models: OCR text recognition support')
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$_srcname-$pkgver"
  cargo build --release --locked --workspace
}

package() {
  cd "$_srcname-$pkgver"

  # Install binary
  install -Dm755 "target/release/jjaeng" "$pkgdir/usr/bin/jjaeng"
  install -Dm755 "target/release/jjaengd" "$pkgdir/usr/bin/jjaengd"
  install -Dm755 "target/release/jjaeng-mcp" "$pkgdir/usr/bin/jjaeng-mcp"
  install -Dm644 "packaging/jjaengd.service" "$pkgdir/usr/lib/systemd/user/jjaengd.service"

  # Install documentation
  install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md" || true
  install -Dm644 "README.ko.md" "$pkgdir/usr/share/doc/$pkgname/README.ko.md" || true
  install -Dm644 "NOTICE" "$pkgdir/usr/share/doc/$pkgname/NOTICE" || true

  # Install dual-license texts
  install -Dm644 "LICENSE-MIT" "$pkgdir/usr/share/licenses/$pkgname/LICENSE-MIT" || true
  install -Dm644 "LICENSE-APACHE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE-APACHE" || true
}
