{ pkgs, ... }:

{
  packages = with pkgs; [
    lld
    cargo-audit
    cargo-deny
    cargo-release
    cargo-watch
  ];

  languages = {
    rust = {
      enable = true;
    };
  };
}
