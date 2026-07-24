#!/usr/bin/env python3
"""Generate short-lived self-signed WebTransport certificates."""

from __future__ import annotations

import argparse
import ipaddress
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


OUTPUT_DIR = Path(__file__).resolve().parent
DEFAULT_SANS = "127.0.0.1,localhost"


def openssl_config(names: list[str]) -> str:
    alternatives: list[str] = []
    dns_index = ip_index = 1
    for name in names:
        try:
            ipaddress.ip_address(name)
        except ValueError:
            alternatives.append(f"DNS.{dns_index} = {name}")
            dns_index += 1
        else:
            alternatives.append(f"IP.{ip_index} = {name}")
            ip_index += 1
    return "\n".join((
        "[req]", "distinguished_name = dn", "prompt = no", "req_extensions = req_ext",
        "[dn]", f"CN = {names[0]}", "[req_ext]", "subjectAltName = @alt_names",
        "[alt_names]", *alternatives, "",
    ))


def parse_sans(value: str) -> list[str]:
    names = [item.strip() for item in value.split(",") if item.strip()]
    if not names:
        raise ValueError("at least one domain or IP address is required")
    if any("\n" in name or "\r" in name or "=" in name for name in names):
        raise ValueError("invalid character in domain or IP address")
    return names


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sans", nargs="?", default=DEFAULT_SANS, help="comma-separated domains and IP addresses")
    args = parser.parse_args(argv)
    openssl = shutil.which("openssl")
    if openssl is None:
        print("Missing openssl. Install it before running.", file=sys.stderr)
        return 1
    try:
        names = parse_sans(args.sans)
        print(f"Generating certificates for {','.join(names)}...")
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".cnf", delete=False) as config:
            config.write(openssl_config(names))
            config_path = Path(config.name)
        try:
            subprocess.run([
                openssl, "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
                "-keyout", str(OUTPUT_DIR / "key.pem"), "-out", str(OUTPUT_DIR / "cert.pem"),
                "-days", "14", "-nodes", "-config", str(config_path), "-extensions", "req_ext",
            ], check=True)
        finally:
            config_path.unlink(missing_ok=True)
        fingerprint = subprocess.run([
            openssl, "x509", "-in", str(OUTPUT_DIR / "cert.pem"), "-noout", "-sha256", "-fingerprint",
        ], check=True, text=True, stdout=subprocess.PIPE).stdout.strip().split("=", 1)[-1].replace(":", "")
        (OUTPUT_DIR / "digest.txt").write_text(fingerprint, encoding="ascii")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        return 1
    print(f"Wrote new fingerprint {fingerprint} to {OUTPUT_DIR / 'digest.txt'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
