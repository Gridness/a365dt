class A365dt < Formula
  desc "Download Anime365 episodes without guessing translations"
  homepage "https://github.com/@REPOSITORY@"
  version "@VERSION@"
  license "Apache-2.0"
  depends_on "ffmpeg-full" => :optional
  on_macos do
    depends_on arch: :arm64
    on_arm do
      url "https://github.com/@REPOSITORY@/releases/download/v@VERSION@/a365dt-v@VERSION@-aarch64-apple-darwin.tar.gz"
      sha256 "@MACOS_ARM64_SHA256@"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/@REPOSITORY@/releases/download/v@VERSION@/a365dt-v@VERSION@-aarch64-unknown-linux-musl.tar.gz"
      sha256 "@LINUX_ARM64_SHA256@"
    end
    on_intel do
      url "https://github.com/@REPOSITORY@/releases/download/v@VERSION@/a365dt-v@VERSION@-x86_64-unknown-linux-musl.tar.gz"
      sha256 "@LINUX_X64_SHA256@"
    end
  end
  def install
    bin.install "a365dt"
  end
  test do
    assert_match version.to_s, shell_output("#{bin}/a365dt --version")
  end
end
