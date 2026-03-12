# Maintainer: Jjaeng contributors
pkgname=jjaeng
pkgver=0.6.0
pkgrel=1
pkgdesc="Hyprland screenshot preview and editor utility"
arch=('x86_64' 'aarch64')
options=(!lto)
url="https://github.com/chllming/Jjaeng"
_srcname="Jjaeng"
license=('MIT' 'Apache-2.0')
depends=('gtk4' 'hyprland' 'grim' 'slurp' 'wl-clipboard')
makedepends=('rust' 'cargo' 'pkgconf' 'gtk4' 'cmake' 'clang' 'git')
optdepends=('jjaeng-ocr-models: OCR text recognition support')
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$_srcname-$pkgver"
  cargo build --release --locked
}

package() {
  cd "$_srcname-$pkgver"

  # Install binary
  install -Dm755 "target/release/jjaeng" "$pkgdir/usr/bin/jjaeng"

  # Install documentation
  install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md" || true
  install -Dm644 "README.ko.md" "$pkgdir/usr/share/doc/$pkgname/README.ko.md" || true
  install -Dm644 "NOTICE" "$pkgdir/usr/share/doc/$pkgname/NOTICE" || true

  # Install dual-license texts
  install -Dm644 "LICENSE-MIT" "$pkgdir/usr/share/licenses/$pkgname/LICENSE-MIT" || true
  install -Dm644 "LICENSE-APACHE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE-APACHE" || true
}
