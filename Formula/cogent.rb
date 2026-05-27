class Cogent < Formula
  desc "Unified security audit & compliance platform"
  homepage "https://github.com/KidIkaros/cogent"
  url "https://github.com/KidIkaros/cogent/archive/refs/tags/v1.0.0.tar.gz"
  # sha256 computed at release time — run: shasum -a 256 cogent-v1.1.0.tar.gz
  sha256 "PLACEHOLDER — update after pushing v1.1.0 tag"
  license "Apache-2.0"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--workspace"

    # Install main binary
    bin.install "target/release/cogent"
    bin.install "target/release/cogent-server"

    # Install all tool binaries
    tools = %w[
      access-control cohesion comments coupling crap cryptocheck
      deadcode debt doccov dupfind errhandle fuzz halstead licenses
      linelen mutate propcov riskmap sast sbom secrets supply-chain
      taint typecov vulnscan
    ]
    tools.each do |tool|
      bin.install "target/release/#{tool}"
    end

    # Install documentation
    (share/"cogent").install "README.md"
    (share/"cogent").install "docs"
  end

  test do
    system "#{bin}/cogent", "--version"
    system "#{bin}/cogent", "check", ".", "--force"
  end
end
