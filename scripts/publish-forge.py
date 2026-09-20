import json
import os
from pathlib import Path
import subprocess
import tempfile


TARGETS = ['x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu', 'aarch64-apple-darwin']


def publish(directory, repository, revision):
    files = [directory / f'rootbeer-forge-{target}' for target in TARGETS]
    if any(not path.is_file() or path.is_symlink() for path in files):
        raise ValueError('Forge publication requires all three regular platform binaries')
    tag = 'forge-' + revision
    result = subprocess.run(['gh', 'release', 'view', tag, '--repo', repository, '--json', 'isDraft'], capture_output=True, text=True)
    if result.returncode:
        if 'release not found' not in result.stderr.lower():
            raise RuntimeError(result.stderr)
        subprocess.run(['gh', 'release', 'create', tag, '--repo', repository, '--target', revision,
                        '--title', 'Forge ' + revision, '--draft', '--prerelease', '--latest=false',
                        '--notes', 'Tested Forge binaries for the pinned engine commit. Verify the GitHub build attestation before execution.'], check=True)
    elif not json.loads(result.stdout)['isDraft']:
        with tempfile.TemporaryDirectory() as temporary:
            for path in files:
                subprocess.run(['gh', 'release', 'download', tag, '--repo', repository,
                                '--pattern', path.name, '--dir', temporary], check=True)
                subprocess.run(['gh', 'attestation', 'verify', str(Path(temporary) / path.name), '--repo', repository,
                                '--signer-workflow', repository + '/.github/workflows/build.yml',
                                '--source-ref', 'refs/heads/main', '--source-digest', revision,
                                '--deny-self-hosted-runners'], check=True)
        return
    subprocess.run(['gh', 'release', 'upload', tag, '--repo', repository, '--clobber', *map(str, files)], check=True)
    subprocess.run(['gh', 'release', 'edit', tag, '--repo', repository, '--draft=false', '--prerelease', '--latest=false'], check=True)


if __name__ == '__main__':
    publish(Path('forge'), os.environ['GITHUB_REPOSITORY'], os.environ['GITHUB_SHA'])
