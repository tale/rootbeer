"""Exercise catalog installs and locked offline replay in an isolated profile."""

import argparse
import hashlib
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
                if recipe.get("build"):
                    destination = root / "artifact"
                    subprocess.run(
                        [rb, "package", "build", name + "@" + version, "--output", destination],
                        env=environment, check=True, timeout=1500,
                    )
                    script = destination / "install.lua"
                command = [rb, "apply", "--script", script]
                subprocess.run(command, env=environment, check=True, timeout=300)
                lock_path = script.parent / "rootbeer.lock"
                lock = lock_path.read_bytes()
                if recipe.get("build"):
                    receipt = json.loads((script.parent / "receipt.json").read_bytes())
                    assert receipt["build"] == recipe["build"]
                    assert receipt["revision"] == recipe["revision"]
                    assert receipt["package"]["name"] == name
                    bundle = root / "bundle"
                    subprocess.run(
                        [rb, "package", "bundle", "--receipt", script.parent / "receipt.json",
                         "--base-url", "https://packages.example/rootbeer", "--output", bundle],
                        env=environment, check=True, timeout=120,
                    )
                    index_bytes = (bundle / "index.json").read_bytes()
                    index = json.loads(index_bytes)
                    published = index["artifacts"][name + "@" + version][system]
                    archive_sha256 = published["package"]["source"]["Url"]["sha256"]
                    assert hashlib.sha256(
                        (bundle / "artifacts" / (archive_sha256 + ".tar.gz")).read_bytes()
                    ).hexdigest() == archive_sha256
                    assert (bundle / "index.sha256").read_text().split()[0] == hashlib.sha256(index_bytes).hexdigest()
                    assert published["package"]["output_sha256"] == receipt["package"]["output_sha256"]
                    downloads = root / "state/rootbeer/downloads"
                    downloads.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(
                        bundle / "artifacts" / (archive_sha256 + ".tar.gz"),
                        downloads / ("sha256-" + archive_sha256),
                    )
                    script = root / "published" / "init.lua"
                    script.parent.mkdir()
                    script.write_text(
                        'local rb = require("rootbeer")\n'
                        'rb.package_index({url = '
                        + json.dumps((bundle / "index.json").as_uri())
                        + ', sha256 = ' + json.dumps(hashlib.sha256(index_bytes).hexdigest())
                        + '})\n' + f"rb.package({json.dumps(name + '@' + version)})\n"
                    )
                    command = [rb, "apply", "--script", script]
                    subprocess.run(command, env=environment, check=True, timeout=120)
                    lock_path = script.parent / "rootbeer.lock"
                    lock = lock_path.read_bytes()
                    proof = next(iter(json.loads(lock)["resolutions"].values()))["proof"]
                    assert proof["type"] == "published_index"
                    assert proof["index"]["sha256"] == hashlib.sha256(index_bytes).hexdigest()
                    shutil.rmtree(bundle)
                    (downloads / ("sha256-" + hashlib.sha256(index_bytes).hexdigest())).unlink()
                else:
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
                assert lock_path.read_bytes() == lock
                assert all((bins / name).is_file() for name in recipe["bins"])
                print(f"PASS {name}@{version}: commands, provenance, and offline reinstall", flush=True)


if __name__ == "__main__":
    main()
