"""Pure native admission for canonical common semantic and direct v2 contracts.

No transport, credential lookup, decoder, action execution or persistence lives
here. The existing pinned jevkit client remains the only provider client. Image
metadata is never promoted to a decoder proof or native image permission.
"""
from __future__ import annotations

import base64
import hashlib
import json
import math
import re

POLICY = 'jev.direct-selected-answer.v2'
CLIENT_SHA = '63915a3925e074b3d314c65bd2eaf4146523c7b3e3692dd0814440c739493f5c'


class ContractHeld(Exception):
    pass


def need(condition, code):
    if not condition:
        raise ContractHeld(code)


def numeric(value):
    try:
        return type(value) in (int, float) and math.isfinite(value)
    except OverflowError:
        return False


def probability(value):
    return numeric(value) and 0 <= value <= 1


def identifier(value):
    return (isinstance(value, str) and 0 < len(value) <= 1024 and bool(value.strip())
            and not re.search(r'[\x00-\x1f]', value)
            and value not in ('__proto__', 'constructor', 'prototype'))


def content(value, nullable=True):
    return (nullable and value is None) or isinstance(value, (str, list, dict))


def _number(value):
    """ECMAScript JSON number spelling for the binary64 common source contract."""
    try:
        value = float(value)
    except OverflowError:
        raise ContractHeld('v2_nonfinite_number') from None
    need(math.isfinite(value), 'v2_nonfinite_number')
    if value == 0:
        return '0'
    negative = value < 0
    raw = repr(abs(value)).lower()
    mantissa, _, exponent = raw.partition('e')
    exponent = int(exponent or 0)
    before, _, after = mantissa.partition('.')
    digits = (before + after).lstrip('0')
    position = len(before) + exponent
    # Leading zeroes in a fractional repr do not count as significant digits.
    position -= len(before + after) - len((before + after).lstrip('0'))
    digits = digits.rstrip('0')
    if 0 < position <= 21:
        result = (digits + '0' * (position - len(digits)) if position >= len(digits)
                  else digits[:position] + '.' + digits[position:])
    elif -6 < position <= 0:
        result = '0.' + '0' * -position + digits
    else:
        exponent = position - 1
        result = digits[0] + ('.' + digits[1:] if len(digits) > 1 else '')
        result += 'e' + ('+' if exponent >= 0 else '') + str(exponent)
    return ('-' if negative else '') + result


def canonical_json(value, depth=0):
    """Match common canonicalJson, including UTF-16 and JS integer-key order."""
    need(depth <= 32, 'v2_json_depth')
    if value is None:
        return 'null'
    if type(value) is bool:
        return 'true' if value else 'false'
    if type(value) in (int, float):
        return _number(value)
    if isinstance(value, str):
        rendered = json.dumps(value, ensure_ascii=False)
        return ''.join('\\u%04x' % ord(c) if 0xd800 <= ord(c) <= 0xdfff else c for c in rendered)
    if isinstance(value, list):
        need(len(value) <= 1024, 'v2_json_array_bound')
        return '[' + ','.join(canonical_json(v, depth + 1) for v in value) + ']'
    need(isinstance(value, dict) and len(value) <= 1024
         and all(isinstance(k, str) and k not in ('__proto__', 'constructor', 'prototype')
                 for k in value), 'v2_json_object')
    keys = sorted(value, key=lambda k: k.encode('utf-16-be', 'surrogatepass'))
    indices = [k for k in keys if k.isascii() and k.isdigit() and str(int(k)) == k
               and int(k) < 4294967295]
    keys = sorted(indices, key=int) + [k for k in keys if k not in indices]
    return '{' + ','.join(canonical_json(k, depth + 1) + ':' + canonical_json(value[k], depth + 1)
                          for k in keys) + '}'


def digest(value):
    return hashlib.sha256(canonical_json(value).encode('utf-8')).hexdigest()


def strings_in(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for child in value.values():
            yield from strings_in(child)
    elif isinstance(value, list):
        for child in value:
            yield from strings_in(child)


def validate_question(question):
    need(isinstance(question, dict) and 'instructions' in question
         and content(question['instructions']), 'v2_question_content')
    kind = question.get('type')
    need(kind in ('choice', 'noul', 'score', 'box'), 'v2_question_type')
    allowed = {'type', 'instructions'} | ({'criteria'} if kind != 'box' else set())
    need(set(question) <= allowed, 'v2_question_fields')
    criteria = question.get('criteria')
    if kind == 'choice':
        need(isinstance(criteria, dict) and 1 <= len(criteria) <= 255
             and all(identifier(k) and content(v) for k, v in criteria.items()), 'v2_choice_criteria')
    elif kind == 'noul' and 'criteria' in question:
        need(isinstance(criteria, dict) and set(criteria) <= {'true', 'false'}
             and all(content(v) for v in criteria.values()), 'v2_noul_criteria')
    elif kind == 'score':
        need(isinstance(criteria, list) and 1 <= len(criteria) <= 10
             and all(content(v, False) for v in criteria), 'v2_score_criteria')


def validate_request(request):
    need(request.get('consumptionPolicy') == POLICY and type(request.get('imageOptIn')) is bool,
         'v2_consumption_policy_or_opt_in')
    canonical_json(request)
    need(request['stateDigest'] == digest(request['state']), 'v2_state_digest_mismatch')
    for name, question in request['questions'].items():
        need(identifier(name), 'v2_question_id')
        validate_question(question)
    assets = request.get('assets', {})
    need(isinstance(assets, dict) and ('assets' not in request or bool(assets)), 'v2_assets_nonempty')
    need(('semanticBoundary' in request['state']) != ('directBoundary' in request['state']),
         'v2_exactly_one_boundary_required')
    if 'semanticBoundary' in request['state']:
        boundary = request['state']['semanticBoundary']
        expected = {'schema': 'jev.semantic-boundary.v2', 'mode': 'advice-only',
                    'executionAuthorityGranted': False, 'automaticWrite': False,
                    'workerCompletionVerified': False}
        # Canonical equality also rejects numeric 0 in place of boolean false.
        need(canonical_json(boundary) == canonical_json(expected), 'v2_semantic_boundary_required')
        need(request['imageOptIn'] is False and 'assets' not in request
             and all(q['type'] != 'box' for q in request['questions'].values()),
             'v2_semantic_text_only')
        return
    boundary = request['state'].get('directBoundary')
    need(isinstance(boundary, dict) and set(boundary) == {
        'opportunity', 'actions', 'boxes', 'imageEvidence'}, 'v2_direct_boundary_required')
    boxes, evidence = boundary['boxes'], boundary['imageEvidence']
    need(isinstance(boxes, dict) and isinstance(evidence, dict) and set(evidence) == set(assets),
         'v2_image_evidence_coverage')
    need(isinstance(boundary['actions'], list) and len(boundary['actions']) <= 31
         and isinstance(boundary['opportunity'], dict), 'v2_boundary_shape')
    if assets or boxes or any(q['type'] == 'box' for q in request['questions'].values()):
        need(request['imageOptIn'] is True, 'v2_experimental_images_opt_in_required')
    # directBoundary is protocol metadata, not represented semantic image content.
    state = {k: v for k, v in request['state'].items() if k != 'directBoundary'}
    question_content = [[q['instructions'], q.get('criteria')] for q in request['questions'].values()]
    represented = list(strings_in([state, request['scope'], question_content]))
    budget_strings = list(strings_in([request['state'], request['scope'], question_content]))
    parts, processed, expanded = set(), 0, len(canonical_json([request['state'], request['questions']]).encode())
    reference_bytes = 0
    for ref, asset in assets.items():
        need(identifier(ref) and len(ref.encode()) <= 1024
             and not any(other != ref and (other in ref or ref in other) for other in assets),
             'v2_image_reference_invalid')
        reference_bytes += len(ref.encode())
        need(any(ref in text for text in represented), 'v2_image_not_represented')
        need(isinstance(asset, dict) and asset.get('type') == 'image', 'v2_image_type')
        item = evidence[ref]
        need(isinstance(item, dict) and item.get('decoded') is True and item.get('still') is True
             and all(type(item.get(k)) is int for k in ('width', 'height', 'byteLength', 'processedByteLength'))
             and 280 <= item['width'] <= 1568 and 280 <= item['height'] <= 1568
             and 78400 <= item['width'] * item['height'] <= 2458624
             and 0 < item['byteLength'] <= 16777216 and item['processedByteLength'] > 0
             and item.get('format') in ('png', 'jpg', 'jpeg'), 'v2_image_owner_geometry_or_bytes')
        source = item.get('source')
        need(isinstance(source, dict) and identifier(source.get('path'))
             and isinstance(item.get('sha256'), str) and re.fullmatch('[a-f0-9]{64}', item['sha256'])
             and source.get('sha256') == item['sha256']
             and isinstance(item.get('evidenceRefs'), list) and 1 <= len(item['evidenceRefs']) <= 128
             and all(identifier(r) for r in item['evidenceRefs']), 'v2_image_owner_source')
        if 'part' in asset:
            need(set(asset) <= {'type', 'part', 'format'} and identifier(asset['part'])
                 and asset['part'] != 'request' and asset['part'] not in parts, 'v2_multipart_asset')
            parts.add(asset['part'])
        else:
            need(set(asset) == {'type', 'format', 'data_base64'}
                 and asset['format'] in ('png', 'jpg', 'jpeg')
                 and isinstance(asset['data_base64'], str) and bool(asset['data_base64']), 'v2_base64_asset')
            try:
                raw = base64.b64decode(asset['data_base64'], validate=True)
            except (ValueError, TypeError):
                raise ContractHeld('v2_base64_asset') from None
            need(base64.b64encode(raw).decode() == asset['data_base64']
                 and len(raw) == item['byteLength']
                 and hashlib.sha256(raw).hexdigest() == item['sha256'], 'v2_asset_bytes_mismatch')
        if 'format' in asset:
            need(asset['format'] in ('png', 'jpg', 'jpeg')
                 and asset['format'].replace('jpeg', 'jpg') == item['format'].replace('jpeg', 'jpg'),
                 'v2_asset_format_mismatch')
        processed += item['processedByteLength']
        expanded += item['processedByteLength'] * sum(t.count(ref) for t in budget_strings)
    need(reference_bytes <= 65536 and processed <= 33554432 and expanded <= 33554432,
         'v2_image_request_bound')
    need(set(boxes) == {k for k, q in request['questions'].items() if q['type'] == 'box'},
         'v2_box_binding_required')
    for name, binding in boxes.items():
        need(isinstance(binding, dict) and set(binding) == {'assetRef', 'presenceQuestionId', 'minimumPresence'}
             and isinstance(binding.get('assetRef'), str) and binding['assetRef'] in assets
             and isinstance(binding.get('presenceQuestionId'), str)
             and request['questions'].get(binding['presenceQuestionId'], {}).get('type') == 'noul'
             and probability(binding.get('minimumPresence')) and binding['minimumPresence'] > 0.5,
             'v2_box_presence_binding')


def admit_existing_client(request):
    """Pinned client subset; execute before credential resolution or transport.

    Full decoder and multipart framing do not exist in the pinned owner source.
    Metadata and base64 syntax alone cannot admit images or Box.
    """
    if any('part' in a for a in request.get('assets', {}).values()):
        raise ContractHeld('v2_multipart_owner_extension_required')
    if request.get('assets') or any(q['type'] == 'box' for q in request['questions'].values()):
        raise ContractHeld('v2_image_decoder_owner_extension_required')
    for name, q in request['questions'].items():
        if q['type'] in ('choice', 'score') and len(q['criteria']) == 1:
            raise ContractHeld('v2_single_criterion_client_extension_required')
        if not q['instructions'] or (isinstance(q['instructions'], str) and not q['instructions'].strip()):
            raise ContractHeld('v2_question_content_client_extension_required')
        if isinstance(q['instructions'], str) and re.sub(r'[\W_]+', ' ', q['instructions'].lower()).strip() == re.sub(
                r'[\W_]+', ' ', name.lower()).strip():
            raise ContractHeld('v2_question_identity_client_extension_required')
        if q['type'] == 'noul' and 'criteria' in q and (not q['criteria'] or any(
                not v or isinstance(v, str) and not v.strip() for v in q['criteria'].values())):
            raise ContractHeld('v2_noul_content_client_extension_required')
        if q['type'] == 'score' and any(not v or isinstance(v, str) and not v.strip() for v in q['criteria']):
            raise ContractHeld('v2_score_content_client_extension_required')


def distribution(value, keys):
    need(isinstance(value, dict) and set(value) == set(keys)
         and all(probability(v) for v in value.values())
         and abs(sum(value.values()) - 1) <= 0.03, 'v2_answer_distribution')
    return value


def validate_answers(request, answers):
    """Validate raw provider fields, never client defaults, clamps or projections.

    Box stays raw technical evidence. The canonical common consumer gates its use
    on the separately bound Noul presence result; this module grants no action.
    """
    need(isinstance(answers, dict) and set(answers) == set(request['questions']), 'v2_answer_coverage')
    for name, q in request['questions'].items():
        a = answers[name]
        need(isinstance(a, dict) and a.get('type') == q['type'], 'v2_answer_type')
        if q['type'] == 'noul':
            need(set(a) == {'type', 'noul'} and probability(a.get('noul')), 'v2_noul_range')
        elif q['type'] == 'choice':
            need(set(a) == {'type', 'choice', 'confidence', 'probabilities'}, 'v2_choice_fields')
            p = distribution(a.get('probabilities'), q['criteria'])
            need(identifier(a.get('choice')) and a['choice'] in q['criteria']
                 and probability(a.get('confidence')) and p[a['choice']] == max(p.values()),
                 'v2_choice_membership')
        elif q['type'] == 'score':
            need(set(a) == {'type', 'score', 'confidence', 'legend', 'probabilities'}, 'v2_score_fields')
            legend = {str(i): value for i, value in enumerate(q['criteria'])}
            p = distribution(a.get('probabilities'), legend)
            need(canonical_json(a.get('legend')) == canonical_json(legend)
                 and probability(a.get('confidence')) and numeric(a.get('score'))
                 and 0 <= a['score'] <= len(legend) - 1
                 and abs(a['score'] - sum(int(i) * v for i, v in p.items())) <= 0.03,
                 'v2_score_value_or_legend')
        else:
            edges = a.get('box')
            need(set(a) == {'type', 'box', 'confidence'} and isinstance(edges, dict)
                 and set(edges) == {'top', 'bottom', 'left', 'right'}
                 and all(probability(v) for v in edges.values())
                 and edges['top'] < edges['bottom'] and edges['left'] < edges['right']
                 and (a.get('confidence') is None or probability(a.get('confidence'))), 'v2_box_geometry')
    return answers
