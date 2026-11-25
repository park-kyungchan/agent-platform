#!/usr/bin/env python3
"""The work schema declares only what is used.

Usage: python3 -I -B ci/check-schema-use.py [<repository root>]

For every action type and every view of ontology/work.schema.json it looks for the name as a whole word (an
underscore is part of a word), or for an action also its generated type name (the name in CamelCase, as
`ont schema gen` makes it for crates/ap/build.rs), in the files under crates/ap/src, content, deploy and scripts
and in ontology/migrate. It prints one line `unused action <name>` or `unused view <name>` for each name not
found, in the order of the schema, and exits 1 when there is one, else 0. It takes no list of exceptions: an action
a Lead applies by hand is named in content/runtime-references/ONTOLOGY-USE.md, which is under content.
"""
import json
import os
import re
import sys

PLACES = ('crates/ap/src', 'content', 'deploy', 'scripts', 'ontology/migrate')
SCHEMA = 'ontology/work.schema.json'


def type_name(action):
    """The generated type name of an action: each part between underscores with its first letter in upper case."""
    return ''.join(part[:1].upper() + part[1:] for part in action.split('_'))


def texts(root):
    """The text of every file of the places, read as UTF-8 with undecodable bytes replaced."""
    for place in PLACES:
        path = os.path.join(root, place)
        if os.path.isfile(path):
            files = [path]
        else:
            files = [os.path.join(d, name) for d, _, names in sorted(os.walk(path)) for name in sorted(names)]
        for file in files:
            if os.path.isfile(file) and not os.path.islink(file):
                with open(file, 'rb') as handle:
                    yield handle.read().decode('utf-8', 'replace')


def unused(root):
    with open(os.path.join(root, SCHEMA), encoding='utf-8') as handle:
        schema = json.load(handle)
    words = set()
    for text in texts(root):
        words.update(re.findall(r'\w+', text))
    lines = []
    for action in schema.get('action_types', {}):
        if action not in words and type_name(action) not in words:
            lines.append('unused action ' + action)
    for view in schema.get('views', {}):
        if view not in words:
            lines.append('unused view ' + view)
    return lines


def main(argv):
    if len(argv) > 2:
        print('usage: ci/check-schema-use.py [<repository root>]', file=sys.stderr)
        return 2
    root = argv[1] if len(argv) == 2 else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    lines = unused(root)
    for line in lines:
        print(line)
    return 1 if lines else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
