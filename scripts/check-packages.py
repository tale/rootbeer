"""Exercise catalog installs and locked offline replay in an isolated profile."""

import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rb", type=Path, required=True)
    parser.add_argument("--package", action="append", dest="packages")
    args = parser.parse_args()
    rb = args.rb.resolve(strict=True)
    catalog = json.loads(subprocess.check_output([rb, "package", "index"]))
    architecture = {"arm64": "aarch64", "AMD64": "x86_64"}.get(
        platform.machine(), platform.machine()
    )
    operating_system = {"Darwin": "macos", "Linux": "linux"}[platform.system()]
    system = f"{architecture}-{operating_system}"

    for name in args.packages or catalog["packages"]:
        package = catalog["packages"][name]
        for version, recipe in package["versions"].items():
            if system not in recipe["systems"]:
                continue
            print(f"Checking {name}@{version} on {system}", flush=True)
            with tempfile.TemporaryDirectory(prefix="rootbeer-catalog-") as temporary:
                root = Path(temporary)
                environment = os.environ.copy()
                environment.update(
                    HOME=str(root),
                    XDG_STATE_HOME=str(root / "state"),
                    XDG_DATA_HOME=str(root / "data"),
                    XDG_CONFIG_HOME=str(root / "config"),
                    XDG_CACHE_HOME=str(root / "cache"),
                )
                script = root / "init.lua"
                script.write_text(
                    'local rb = require("rootbeer")\n'
                    f"rb.package({json.dumps(name + '@' + version)})\n"
                )
                command = [rb, "apply", "--script", script]
                subprocess.run(command, env=environment, check=True, timeout=300)
                lock = (root / "rootbeer.lock").read_bytes()
                resolution = next(iter(json.loads(lock)["resolutions"].values()))
                proof = resolution["proof"]
                assert proof["type"] == "catalog"
                assert (proof["name"], proof["version"], proof["revision"]) == (
                    name, version, recipe["revision"]
                )
                assert proof["source_proof"]
                profile = root / "state/rootbeer/profiles/default/current"
                bins = profile / "bin"
                assert set(path.name for path in bins.iterdir()) == set(recipe["bins"])

                test_environment = {
                    "HOME": str(root),
                    "PATH": str(bins) + ":/usr/bin:/bin",
                    "LANG": "C",
                    "TMPDIR": temporary,
                }
                for check in recipe["checks"]:
                    subprocess.run(
                        [str(bins / check[0]), *check[1:]],
                        env=test_environment,
                        cwd=root,
                        check=True,
                        timeout=30,
                    )

                shutil.rmtree(profile)
                shutil.rmtree(root / "state/rootbeer/store")
                subprocess.run(
                    [*command, "--offline"], env=environment, check=True, timeout=60
                )
                assert (root / "rootbeer.lock").read_bytes() == lock
                assert all((bins / name).is_file() for name in recipe["bins"])
                print(f"PASS {name}@{version}: commands, provenance, and offline reinstall", flush=True)


if __name__ == "__main__":
    main()
