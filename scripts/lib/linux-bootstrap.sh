#!/usr/bin/env bash
set -euo pipefail

print_stage() {
  local message="$1"
  printf '[stage] %s\n' "$message"
}

run_privileged() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    echo "need root or sudo to install missing dependencies" >&2
    exit 1
  fi
}

linux_package_for_command() {
  case "$1" in
    curl)
      echo "curl"
      ;;
    tar)
      echo "tar"
      ;;
    sha256sum)
      echo "coreutils"
      ;;
    python3)
      echo "python3"
      ;;
    rpm)
      echo "rpm"
      ;;
    systemctl)
      echo "systemd"
      ;;
    journalctl)
      echo "systemd"
      ;;
    rpm2cpio)
      echo "rpm"
      ;;
    cpio)
      echo "cpio"
      ;;
    wrk)
      echo "wrk"
      ;;
    *)
      echo ""
      ;;
  esac
}

install_linux_packages() {
  local packages=("$@")

  if [[ "${#packages[@]}" -eq 0 ]]; then
    return 0
  fi

  if command -v apt-get >/dev/null 2>&1; then
    print_stage "installing dependencies with apt-get"
    run_privileged apt-get update
    run_privileged apt-get install -y "${packages[@]}"
    return 0
  fi

  if command -v dnf >/dev/null 2>&1; then
    print_stage "installing dependencies with dnf"
    run_privileged dnf install -y "${packages[@]}"
    return 0
  fi

  if command -v yum >/dev/null 2>&1; then
    print_stage "installing dependencies with yum"
    run_privileged yum install -y "${packages[@]}"
    return 0
  fi

  echo "unsupported package manager; please install manually: ${packages[*]}" >&2
  exit 1
}

ensure_linux_commands() {
  local required_commands=("$@")
  local missing_commands=()
  local install_packages=()
  local command_name=""
  local package_name=""

  for command_name in "${required_commands[@]}"; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
      missing_commands+=("$command_name")
      package_name="$(linux_package_for_command "$command_name")"
      if [[ -n "$package_name" ]]; then
        install_packages+=("$package_name")
      fi
    fi
  done

  if [[ "${#missing_commands[@]}" -gt 0 ]]; then
    print_stage "missing commands detected: ${missing_commands[*]}"
    install_linux_packages "${install_packages[@]}"
  fi

  for command_name in "${required_commands[@]}"; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
      echo "required command not found: $command_name" >&2
      exit 1
    fi
  done
}

detect_linux_asset_arch() {
  case "$(uname -m)" in
    x86_64)
      echo "linux-x86_64"
      ;;
    aarch64|arm64)
      echo "linux-arm64"
      ;;
    *)
      echo "unsupported architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac
}
