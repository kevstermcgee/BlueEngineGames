"""Attach portable game information before package checksums and publication.

The carrier describes source and game identity; it grants no publication rights.
External release payloads remain immutable. Only our freshly built launcher or
installer receives executable metadata for those releases.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import shutil
import tempfile
import tomllib
import zipfile

try:
    from . import game_metadata as metadata
except ImportError:
    import game_metadata as metadata

MANIFEST = 'blueengine.game.json'
SLUG = re.compile(r'[a-z0-9]+(?:-[a-z0-9]+)*')


def git(root, *arguments, data=None):
    result = subprocess.run(['git', '-c', 'core.hooksPath=/dev/null', '-C', str(root), *arguments],
                            input=data, capture_output=True, timeout=30)
    if result.returncode:
        raise ValueError('Game metadata needs an initialized, committed source checkout')
    return result.stdout


def json_file(path):
    return json.loads(path.read_text(encoding='utf-8'))


def sha256_file(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def safe_path(root, relative):
    if (not isinstance(relative, str) or not relative or '\\' in relative or ':' in relative
            or any(part in ('', '.', '..') for part in relative.split('/')) or relative.startswith('/')):
        raise ValueError('Unsafe package metadata path')
    path = root / relative
    cursor = root
    for part in relative.split('/'):
        cursor = cursor / part
        if cursor.is_symlink():
            raise ValueError('Package metadata path contains a symlink')
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError('Package metadata path escapes its source directory')
    return path


def repository(root):
    origin = git(root, 'remote', 'get-url', 'origin').decode().strip()
    match = re.fullmatch(r'(?:https://github\.com/|git@github\.com:)([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+?)(?:\.git)?', origin)
    if not match:
        raise ValueError('Game metadata needs a public GitHub repository origin without credentials')
    return match[1]


def committed_inputs(root, paths):
    if git(root, 'status', '--porcelain', '--untracked-files=all', '--', *paths):
        raise ValueError('Commit the selected game, catalog and wiki before creating portable metadata')
    return git(root, 'rev-parse', 'HEAD').decode().strip()


def definition(root, slug):
    if not SLUG.fullmatch(slug):
        raise ValueError('Unsafe game slug')
    catalog = json_file(root / '.games-catalog.json')
    release = json_file(root / '.release-games.json')
    candidates = [game for game in catalog['playables'] + release['data_playables'] + release['native_playables']
                  if game['slug'] == slug]
    if len(candidates) != 1:
        raise ValueError('Game needs one unambiguous release definition')
    return catalog, candidates[0]


def inverse_export(engine, target):
    manifest = json.loads(git(engine, 'show', 'HEAD:games-publish.json'))
    matches = set()
    for category, entries in manifest['collections'].items():
        for entry in entries:
            source = PurePosixPath(entry['source'])
            source_path = safe_path(engine, source.as_posix())
            destination = PurePosixPath(category) / entry.get('destination', '.')
            if source_path.is_file():
                if target == (destination / source.name).as_posix():
                    matches.add(source.as_posix())
            else:
                prefix = destination.as_posix().rstrip('/') + '/'
                if target.startswith(prefix):
                    candidate = source / target[len(prefix):]
                    if safe_path(engine, candidate.as_posix()).is_file():
                        matches.add(candidate.as_posix())
    if len(matches) != 1:
        raise ValueError('The engine export has no unambiguous original source path')
    return matches.pop()


def managed_source(root, engine, catalog, directory, entry):
    prefix = directory.rstrip('/') + '/'
    managed = [item for item in catalog['files'] if item['path'] == directory or item['path'].startswith(prefix)]
    if not managed:
        return None
    if engine is None:
        raise ValueError('Engine-managed metadata needs the exact catalog Engine checkout')
    pin = git(engine, 'rev-parse', 'HEAD').decode().strip()
    if pin != catalog['source_revision'] or repository(engine) != catalog['source_repository']:
        raise ValueError('Engine checkout does not match the frozen catalog source')
    for item in managed:
        blob = git(root, 'show', f"HEAD:{item['path']}")
        if hashlib.sha256(blob).hexdigest() != item['sha256']:
            raise ValueError('A managed game changed without updating its engine export source pin')
    original = inverse_export(engine, entry)
    game_path = str(PurePosixPath(original).parent) if PurePosixPath(original).name in (
        'Cargo.toml', 'game.project.json', 'game.json', 'catalog.json') else original
    return dict(repository=catalog['source_repository'], commit=pin, game_path=game_path)


def build_document(root, slug, engine=None):
    root = Path(root).resolve()
    engine = Path(engine).resolve() if engine is not None else None
    catalog, game = definition(root, slug)
    entry = game.get('entry')
    directory = game.get('directory') or (f'games/{slug}' if (root / 'games' / slug).is_dir()
                                          else str(PurePosixPath(game['files'][0]).parent))
    project = safe_path(root, directory)
    if not project.is_dir():
        raise ValueError('Missing game project for portable metadata')
    if entry is None:
        entry = next((f'{directory}/{name}' for name in ('Cargo.toml', 'game.project.json', 'game.json', 'catalog.json')
                      if (project / name).is_file()), None)
    wiki_path = next((path for path in (root / 'site/wiki' / slug / 'wiki.json', project / 'wiki.json') if path.is_file()), None)
    inputs = ['.games-catalog.json', '.release-games.json', directory, f'site/wiki/{slug}']
    head = committed_inputs(root, inputs)
    wiki = json_file(wiki_path) if wiki_path else None
    identity = json_file(project / 'assets/identity.json') if (project / 'assets/identity.json').is_file() else {}
    cargo = tomllib.loads((project / 'Cargo.toml').read_text(encoding='utf-8')).get('package', {}) if (project / 'Cargo.toml').is_file() else {}
    external = game.get('kind') == 'release-asset'
    external_source = json_file(project / 'release-source.json') if external else None
    if not external and (engine is None or git(engine, 'rev-parse', 'HEAD').decode().strip() != catalog['source_revision']
                         or repository(engine) != catalog['source_repository']):
        raise ValueError('Game metadata needs the exact catalog Engine checkout')
    if not external and git(engine, 'status', '--porcelain', '--untracked-files=all'):
        raise ValueError('Commit or restore Engine source changes before freezing its metadata pin')
    game_json = json_file(project / 'game.json') if (project / 'game.json').is_file() else {}
    version = ((external_source or {}).get('version') or cargo.get('version') or game_json.get('version')
               or (f"schema {game_json['game']}" if 'game' in game_json else 'catalog'))
    if not isinstance(version, str):
        raise ValueError('Game version must be explicitly resolved before packaging')
    description = identity.get('tagline') or cargo.get('description') or (wiki or {}).get('overview') or game['name']
    source = None if external else managed_source(root, engine, catalog, directory, entry) if entry else None
    if not external and source is None:
        source = dict(repository=repository(root), commit=head, game_path=directory)
    engine_pin = None if external else dict(repository=catalog['source_repository'], commit=catalog['source_revision'])
    document = metadata.build_document(project, slug=slug, title=identity.get('title') or game['name'], version=version,
                                       description=description, source=source, engine=engine_pin, wiki=wiki)
    document['game']['controls'] = identity.get('controls') or '\n'.join((wiki or {}).get('controls', []))
    document['posting'] = dict(source_status='unavailable' if external else 'available',
                               note='Source must pass authenticated review and the normal native release gates before publication.')
    document['package'] = dict(repository=repository(root), commit=head, kind=game.get('kind', 'data'),
                              source_status='immutable-external-release' if external else 'committed-source')
    if external:
        document['posting']['source_unavailable_reason'] = 'The pinned external release contains executable assets, not recoverable editable game source.'
        document['package']['external_release'] = external_source
    metadata.canonical_payload(document)
    return document


def entrypoint(stage, slug):
    for name in (f'Play-{slug}.exe', f'{slug}.exe'):
        if (stage / name).is_file():
            return name
    ship = stage / 'ship.json'
    if ship.is_file():
        value = json_file(ship).get('exe')
        if value and safe_path(stage, value).is_file():
            return value
    candidates = [path.name for path in stage.glob('*.exe') if not re.search(r'server|tools|unins|update-', path.name, re.I)]
    if len(candidates) != 1:
        raise ValueError('Cannot identify one playable executable for portable metadata')
    return candidates[0]


def producer_inventories(stage):
    """Check the producer's original bytes before permitting metadata changes."""
    stage = Path(stage)
    result = {}
    for filename in ('ship.json', 'SHA256.json'):
        path = stage / filename
        if not path.exists():
            continue
        if path.is_symlink():
            raise ValueError('Unsafe producer integrity record')
        record = json_file(path)
        hashes = record.get('file_sha256') if filename == 'ship.json' else record
        if not isinstance(hashes, dict) or not hashes:
            raise ValueError('Producer integrity record has no declared payload hashes')
        if filename == 'ship.json':
            files = record.get('files')
            if (not isinstance(files, list) or set(files) != set(hashes) or len(files) != len(hashes)
                    or record.get('exe') not in hashes or record.get('exe_sha256') != hashes[record['exe']]):
                raise ValueError('Producer ship inventory is inconsistent')
        if len({name.casefold() for name in hashes}) != len(hashes):
            raise ValueError('Producer integrity paths are ambiguous')
        for name, digest in hashes.items():
            path = safe_path(stage, name)
            if name.casefold() == filename.casefold() or not isinstance(digest, str) or not re.fullmatch(r'[0-9a-f]{64}', digest):
                raise ValueError('Invalid producer payload hash')
            if not path.is_file() or sha256_file(path) != digest:
                raise ValueError(f'Producer payload changed before metadata embedding: {name}')
        result[filename] = record
    return result


def refresh_producer_inventories(stage, inventories, changed):
    for filename, record in inventories.items():
        hashes = record['file_sha256'] if filename == 'ship.json' else record
        updated = dict(hashes)
        for name in changed:
            if name in updated:
                updated[name] = sha256_file(safe_path(stage, name))
        # Every unchanged payload must still equal the original producer receipt.
        for name in set(hashes) - set(changed):
            if sha256_file(safe_path(stage, name)) != hashes[name]:
                raise ValueError(f'Unchanged producer payload was corrupted: {name}')
        updated[MANIFEST] = sha256_file(stage / MANIFEST)
        if filename == 'ship.json':
            record['files'] = sorted(updated)
            record['file_sha256'] = updated
            previous_exe = record['exe_sha256']
            record['exe_sha256'] = updated[record['exe']]
            record.pop('verified', None)
            record['metadata_repack'] = dict(requires_native_review=True, previous_exe_sha256=previous_exe)
        else:
            record = updated
        (stage / filename).write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')


def stage_package(root, slug, stage, engine=None, extra_executables=(), document=None):
    stage = Path(stage).resolve()
    document = build_document(root, slug, engine) if document is None else document
    metadata.canonical_payload(document)
    if document['game']['slug'] != slug:
        raise ValueError('Portable metadata belongs to a different game')
    executable = entrypoint(stage, slug)
    if document['package']['source_status'] == 'immutable-external-release' and executable != f'Play-{slug}.exe':
        raise ValueError('External release metadata requires our newly compiled Play wrapper; keep the original executable unchanged')
    document['package']['entrypoint'] = executable
    if document['package']['source_status'] == 'immutable-external-release':
        document['package']['external_payload_sha256'] = {
            name: sha256_file(safe_path(stage, name)) for name in document['package']['external_release']['files']}
    extras = list(extra_executables)
    if document['package']['kind'] == 'engine-sandbox' and (stage / 'be2.exe').is_file():
        extras.append('be2.exe')  # Explicitly compiled by our sandbox packaging pipeline.
    owned = list(dict.fromkeys((executable, *extras)))
    external_files = document['package'].get('external_payload_sha256', {})
    launchers = sorted(path.name for path in stage.iterdir()
                       if path.is_file() and path.suffix.lower() in ('.bat', '.cmd') and path.name not in external_files)
    document['package']['playable_executables'] = owned
    document['package']['batch_launchers'] = launchers
    inventories = producer_inventories(stage)
    for name in owned:
        metadata.embed(safe_path(stage, name), document)
    for name in launchers:
        metadata.embed(safe_path(stage, name), document)
    (stage / MANIFEST).write_bytes(metadata.canonical_payload(document))
    refresh_producer_inventories(stage, inventories, {*owned, *launchers, MANIFEST})
    return document


def archive_package(stage, archive):
    document = json_file(Path(stage) / MANIFEST)
    metadata.embed(Path(archive), document)
    if metadata.extract(Path(archive)) != document:
        raise ValueError('ZIP metadata did not survive packaging')
    return document


def legacy_document(row, archive):
    return dict(format='blueengine.game', schema_version=1,
                game=dict(slug=row['slug'], title=row['name'], version=row.get('game_version') or 'unversioned',
                          description=row.get('description') or row['name'], controls='', authors=[], tags=[]),
                source=None, engine=None,
                posting=dict(source_status='unavailable', note='Legacy archive lacks portable source metadata; recover source before submitting a new native revision.'),
                package=dict(kind='legacy-archive', release=row.get('release', ''),
                             archive_sha256=sha256_file(archive),
                             metadata_limitation='The original immutable archive and executable have no portable metadata.'))


def restore_archive(archive, stage, row=None):
    document = metadata.extract(Path(archive))
    if document is None:
        if row is None:
            raise ValueError('Current game ZIP has no portable metadata; rebuild it before installer packaging')
        document = legacy_document(row, archive)
    metadata.canonical_payload(document)
    destination = Path(stage) / MANIFEST
    if destination.exists() and json_file(destination) != document:
        raise ValueError('Unpacked ZIP manifest disagrees with its portable metadata')
    destination.write_bytes(metadata.canonical_payload(document))
    return document


def verify_payload(document, directory):
    directory = Path(directory)
    if json_file(directory / MANIFEST) != document:
        raise ValueError('Installed or staged manifest differs from the immutable package')
    for name in document.get('package', {}).get('playable_executables', []):
        if metadata.extract(safe_path(directory, name)) != document:
            raise ValueError('Playable executable portable metadata differs from its ZIP')
    for name in document.get('package', {}).get('batch_launchers', []):
        if metadata.extract(safe_path(directory, name)) != document:
            raise ValueError('BAT or CMD portable metadata differs from its ZIP')
    for name, digest in document.get('package', {}).get('external_payload_sha256', {}).items():
        if sha256_file(safe_path(directory, name)) != digest:
            raise ValueError('Immutable external executable changed during packaging')
    producer_inventories(directory)


def audit_dist(dist, *, installed=None, slug=None):
    dist = Path(dist)
    with (dist / 'Games-catalog.tsv').open(encoding='utf-8-sig', newline='') as stream:
        rows = list(csv.DictReader(stream, delimiter='\t'))
    if not rows:
        raise ValueError('No game packages to audit')
    checksums = {}
    for line in (dist / 'SHA256SUMS.txt').read_text(encoding='ascii').splitlines():
        digest, name = line.split('  ', 1)
        if name in checksums:
            raise ValueError('Ambiguous package checksum')
        checksums[name] = digest
    proof = []
    for row in rows:
        game_slug = row['slug']
        if slug is not None and game_slug != slug:
            continue
        if not SLUG.fullmatch(game_slug):
            raise ValueError('Unsafe package audit slug')
        archive = dist / f'{game_slug}-windows-x64.zip'
        installer = dist / f'{game_slug}-setup-windows-x64.exe'
        document = metadata.extract(archive)
        if document is None or metadata.extract(installer) != document:
            raise ValueError('ZIP and installer do not carry identical game metadata')
        if document['game']['slug'] != game_slug or document['game']['version'] != row['game_version']:
            raise ValueError('Embedded game identity differs from its published catalog')
        archive_hash, installer_hash = sha256_file(archive), sha256_file(installer)
        if (row['sha256'] != archive_hash or checksums.get(archive.name) != archive_hash
                or checksums.get(installer.name) != installer_hash):
            raise ValueError('Package checksum was computed before metadata embedding')
        with tempfile.TemporaryDirectory(prefix='game-metadata-audit-') as temporary:
            directory = Path(temporary)
            with zipfile.ZipFile(archive) as bundle:
                names = [MANIFEST, *document['package']['playable_executables'], *document['package']['batch_launchers']]
                names.extend(document['package'].get('external_payload_sha256', {}))
                for record_name in ('ship.json', 'SHA256.json'):
                    if record_name in bundle.namelist():
                        names.append(record_name)
                        record = json.loads(bundle.read(record_name))
                        names.extend(record['file_sha256'] if record_name == 'ship.json' else record)
                for name in set(names):
                    destination = safe_path(directory, name)
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with bundle.open(name) as source, destination.open('wb') as target:
                        shutil.copyfileobj(source, target)
            verify_payload(document, directory)
        if installed is not None:
            if slug != game_slug:
                raise ValueError('Installed metadata verification needs an exact selected game')
            verify_payload(document, Path(installed))
        proof.append(dict(slug=game_slug, version=document['game']['version'], zip_sha256=archive_hash,
                          installer_sha256=installer_hash, source=document['source'], installed=installed is not None))
    if not proof:
        raise ValueError('Selected game has no package metadata receipt')
    return dict(schema_version=1, games=proof)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    create = commands.add_parser('document')
    create.add_argument('--root', type=Path, required=True)
    create.add_argument('--slug', required=True)
    create.add_argument('--engine-root', type=Path)
    create.add_argument('--output', type=Path, required=True)
    stage = commands.add_parser('stage')
    stage.add_argument('--root', type=Path, required=True)
    stage.add_argument('--slug', required=True)
    stage.add_argument('--stage', type=Path, required=True)
    stage.add_argument('--engine-root', type=Path)
    stage.add_argument('--extra-exe', action='append', default=[])
    stage.add_argument('--document', type=Path)
    archive = commands.add_parser('archive')
    archive.add_argument('--stage', type=Path, required=True)
    archive.add_argument('--archive', type=Path, required=True)
    restore = commands.add_parser('restore')
    restore.add_argument('--archive', type=Path, required=True)
    restore.add_argument('--stage', type=Path, required=True)
    restore.add_argument('--legacy-row', type=Path)
    installer = commands.add_parser('installer')
    installer.add_argument('--stage', type=Path, required=True)
    installer.add_argument('--installer', type=Path, required=True)
    audit = commands.add_parser('audit')
    audit.add_argument('--dist', type=Path, required=True)
    audit.add_argument('--slug')
    audit.add_argument('--installed', type=Path)
    audit.add_argument('--output', type=Path)
    args = parser.parse_args()
    try:
        if args.command == 'document':
            args.output.write_bytes(metadata.canonical_payload(build_document(args.root, args.slug, args.engine_root)))
        elif args.command == 'stage':
            stage_package(args.root, args.slug, args.stage, args.engine_root, args.extra_exe,
                          json_file(args.document) if args.document else None)
        elif args.command == 'archive':
            archive_package(args.stage, args.archive)
        elif args.command == 'restore':
            restore_archive(args.archive, args.stage, json_file(args.legacy_row) if args.legacy_row else None)
        elif args.command == 'installer':
            document = json_file(args.stage / MANIFEST)
            metadata.embed(args.installer, document, allow_inno=True)
            if metadata.extract(args.installer) != document:
                raise ValueError('Installer portable metadata differs from its packaged game')
        else:
            document = audit_dist(args.dist, installed=args.installed, slug=args.slug)
            if args.output:
                args.output.write_text(json.dumps(document, indent=2) + '\n', encoding='utf-8')
            print(json.dumps(dict(valid=True, games=len(document['games']), installed=args.installed is not None)))
    except (ValueError, OSError, KeyError) as error:
        parser.exit(1, f'Portable game metadata failed: {error}\n')


if __name__ == '__main__':
    main()
