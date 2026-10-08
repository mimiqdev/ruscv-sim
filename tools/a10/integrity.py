"""Strict finite JSON and content-addressed local retrieval; no cleanup."""
import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat

class Invalid(ValueError):
    code = 1
class Unavailable(RuntimeError):
    code = 2

def require(condition, message):
    if not condition:
        raise Invalid(message)

def pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate/conflicting JSON key: ' + key)
        result[key] = value
    return result

def loads(data):
    def constant(value):
        raise Invalid('non-finite JSON: ' + value)
    try:
        value = json.loads(data, object_pairs_hook=pairs, parse_constant=constant)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise Invalid('malformed JSON: ' + str(error)) from error
    finite(value)
    return value

def finite(value):
    if type(value) is float:
        require(math.isfinite(value), 'NaN/infinite/overflowed JSON number')
    elif type(value) is list:
        for item in value:
            finite(item)
    elif type(value) is dict:
        for item in value.values():
            finite(item)

def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False, allow_nan=False).encode()

def digest(data):
    return hashlib.sha256(data).hexdigest()

def safe(root, reference):
    require(type(reference) is str and reference and '\\' not in reference, 'invalid relative artifact reference')
    path = Path(reference)
    require(not path.is_absolute() and all(p not in ('', '.', '..') for p in reference.split('/')), 'absolute/escaping/alias artifact reference')
    current = Path(root)
    require(not current.is_symlink() and not any(p.is_symlink() for p in current.parents), 'symlink bundle root/ancestor')
    for part in path.parts:
        current /= part
        require(not current.is_symlink(), 'symlink-controlled artifact reference')
    return current

def read(root, reference):
    safe(root, reference)  # lexical/early diagnostics; descriptor walk is authority
    require(hasattr(os,'O_NOFOLLOW') and hasattr(os,'O_DIRECTORY'), 'safe artifact retrieval requires descriptor-relative no-follow OS support')
    # Anchor every ancestor and component with O_NOFOLLOW. A concurrent rename
    # cannot make a checked path follow a subsequently swapped symlink/escape.
    directory = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    try:
        components = list(Path(root).absolute().parts[1:]) + list(Path(reference).parts)
        for component in components[:-1]:
            next_dir = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = next_dir
        descriptor = os.open(components[-1],os.O_RDONLY | os.O_NOFOLLOW,dir_fd=directory)
        with os.fdopen(descriptor,'rb') as file:
            require(stat.S_ISREG(os.fstat(file.fileno()).st_mode),'artifact is not a regular file: '+reference)
            return file.read()
    finally:
        os.close(directory)

def write_new(path, data):
    path = Path(path)
    require(not path.is_symlink() and not path.exists(), 'artifact ID overwrite: ' + str(path))
    path.parent.mkdir(parents=True, exist_ok=True)
    # Caller has guarded output; recheck ancestors before the exclusive create.
    require(not any(p.is_symlink() for p in path.parents), 'symlink output ancestor')
    with path.open('xb') as file:
        file.write(data)
        file.flush()  # Reporting failure is not success; no fsync claim.

def json_new(path, value):
    write_new(path, canonical(value) + b'\n')

def seal(root, run_id):
    root = Path(root)
    files = []
    for path in sorted(root.rglob('*')):
        require(not path.is_symlink(), 'cannot seal a symlink')
        if path.is_file():
            name = path.relative_to(root).as_posix()
            # Cargo scratch/caches are not integrity/retention artifacts. Used
            # binaries are copied into evidence/bin; exact transcripts retained.
            if name.startswith('cargo/'):
                continue
            require(name not in ('bundle.json', 'bundle.sha256'), 'bundle ID reuse')
            data = path.read_bytes()
            files.append({'path': name, 'bytes': len(data), 'sha256': digest(data)})
    manifest = {'schema': 'ruscv-artifacts/1', 'id': run_id, 'files': files,
                'integrity': 'checksums, not a cryptographic signer/host attestation; immutable exclusive creation',
                'self_hash': 'manifest excluded; external bundle.sha256 hashes its final bytes'}
    json_new(root / 'bundle.json', manifest)
    write_new(root / 'bundle.sha256', (digest((root / 'bundle.json').read_bytes()) + '\n').encode())
    for entry in files:
        safe(root, entry['path']).chmod(0o444)
    (root / 'bundle.json').chmod(0o444)
    (root / 'bundle.sha256').chmod(0o444)
    return manifest

def retrieve(root, expected_digest=None):
    raw = read(root, 'bundle.json')
    claimed = read(root, 'bundle.sha256').decode().strip()
    require(re.fullmatch('[0-9a-f]{64}', claimed) is not None and digest(raw) == claimed, 'bundle digest mismatch')
    if expected_digest is not None:
        require(claimed == expected_digest, 'external bundle digest mismatch')
    bundle = loads(raw)
    require(set(bundle) == {'schema', 'id', 'files', 'integrity', 'self_hash'} and bundle['schema'] == 'ruscv-artifacts/1', 'unknown bundle version/fields')
    names = set()
    for entry in bundle['files']:
        require(type(entry) is dict and set(entry) == {'path', 'bytes', 'sha256'}, 'artifact identity shape')
        require(entry['path'] not in names and entry['path'] not in ('bundle.json', 'bundle.sha256'), 'duplicate/self artifact ID')
        names.add(entry['path'])
        data = read(root, entry['path'])
        require(type(entry['bytes']) is int and 0 <= entry['bytes'] <= 2**64-1 and len(data) == entry['bytes'], 'artifact size/type mismatch')
        require(digest(data) == entry['sha256'], 'artifact bytes/hash mismatch: ' + entry['path'])
    require({'report.json', 'setup.json', 'raw-report.json', 'evidence/source-manifest.json', 'evidence/commit', 'fixtures/build.json'} <= names, 'missing mandatory bundle artifacts')
    return bundle
