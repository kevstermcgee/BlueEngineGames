"""Stage an existing public game release using a pinned archive digest."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import urllib.request
import zipfile


def stage(source, archive, destination):
    if hashlib.sha256(archive).hexdigest() != source['sha256']:
        raise ValueError('Release archive SHA256 differs from the pinned source')
    with zipfile.ZipFile(io.BytesIO(archive)) as bundle:
        seen = set()
        for entry in bundle.infolist():
            name = entry.filename
            if '\\' in name or ':' in name or name.startswith('/') or any(
                p in ('', '.', '..') for p in name.rstrip('/').split('/')
            ) or name.casefold() in seen:
                raise ValueError(f'Unsafe or duplicate release archive entry: {name}')
            seen.add(name.casefold())
        payload = {}
        for name in source['files']:
            if name not in bundle.namelist() or '/' in name or '\\' in name or ':' in name:
                raise ValueError(f'Missing or unsafe declared release file: {name}')
            payload[name] = bundle.read(name)
        executable = payload[source['executable']]
        if not executable.startswith(b'MZ'):
            raise ValueError('Release executable is not a Windows executable')
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=False)
    for name, contents in payload.items():
        (destination / name).write_bytes(contents)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--archive', type=Path, help='Verify a locally downloaded copy')
    args = parser.parse_args()
    source = json.loads(args.source.read_text())
    if args.archive:
        data = args.archive.read_bytes()
    else:
        with urllib.request.urlopen(source['url'], timeout=60) as response:
            data = response.read()
    stage(source, data, args.output)
