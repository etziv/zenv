#!/usr/bin/env python3
"""figrest: build a whole FigCraft document as Figma-REST file JSON in Python, then open it in ONE call.

Why: drawing node by node over MCP costs one model round-trip per step (minutes per screen). A generated
REST JSON opens in one `open_file` / `figcraft-cli convert` call (thousands of layers in seconds).

Usage (see ../SKILL.md "Bulk build"):
    import sys; sys.path.insert(0, '<skill>/scripts'); from figrest import *
    btn = comp('Button/Primary', F('Button/Primary', 96, 32, [T('Label', 13, '#FFFFFF', 'Lato', 600,
               name='label')], fill='#15181C', layout='HORIZONTAL', pad=(0, 16, 0, 16), main='CENTER',
               cross='CENTER', radius=6, hug='main'))
    screen = F('Login', 1440, 900, [at(inst('Button/Primary', {'label': 'Masuk'}), 100, 100)], fill='#F5F5F4')
    write_document('out.json', 'My file', [page('Components', [btn]), page('Screens', [screen])])

Coordinates: x/y RELATIVE to the parent (as in FigCraft); `finalize` turns them into the absolute boxes the REST
format needs. Auto-layout children get a naive stack; FigCraft lays them out properly on the first edit after
import (any node.set, e.g. the icon batch). Vectors import EMPTY (REST JSON without geometry): give each vector a
unique name and set its SVG path after import with node.set {path} (vector() records the path for that batch).
"""
import copy
import json
import os
import re
import xml.etree.ElementTree as ET

_n = [0]
COMPONENTS = {}      # name -> component node
VECTOR_PATHS = {}    # vector layer name -> SVG path (apply after import: see vector_batch)


def nid():
    _n[0] += 1
    return f'1:{_n[0]}'


def col(h):
    """'#RRGGBB' or '#RRGGBBAA' -> REST colour."""
    h = h.lstrip('#')
    a = int(h[6:8], 16) / 255 if len(h) == 8 else 1
    return {'r': int(h[0:2], 16) / 255, 'g': int(h[2:4], 16) / 255, 'b': int(h[4:6], 16) / 255, 'a': a}


def paints(h):
    return [] if h is None else [{'type': 'SOLID', 'color': col(h)}]


def est_w(s, size, mono=False):
    """Rough text width; FigCraft re-measures auto-width text, so this only seeds the box."""
    return max(1.0, len(s) * size * (0.6 if mono else 0.53))


def T(s, size, color, font, weight=400, w=None, x=0, y=0, name=None, align='LEFT', lh=None):
    """Text. w=None hugs its content (WIDTH_AND_HEIGHT); a w wraps at that width (HEIGHT)."""
    lh = lh or round(size * 1.35)
    width = w or max(est_w(line, size, 'Mono' in font) for line in s.split('\n'))
    return {'id': nid(), 'type': 'TEXT', 'name': name or s.split('\n')[0][:40], 'characters': s,
            '_x': x, '_y': y, '_w': width, '_h': lh * (s.count('\n') + 1),
            'style': {'fontFamily': font, 'fontWeight': weight, 'fontSize': size, 'lineHeightPx': lh,
                      'textAlignHorizontal': align, 'textAutoResize': 'HEIGHT' if w else 'WIDTH_AND_HEIGHT'},
            'fills': paints(color)}


def R(w, h, fill=None, stroke=None, x=0, y=0, radius=0, name='Rectangle', sw=1):
    return {'id': nid(), 'type': 'RECTANGLE', 'name': name, '_x': x, '_y': y, '_w': w, '_h': h,
            'fills': paints(fill), 'strokes': paints(stroke), 'strokeWeight': sw if stroke else 0,
            'strokeAlign': 'INSIDE', 'cornerRadius': radius}


def E(w, h, fill=None, stroke=None, x=0, y=0, name='Ellipse', sw=1):
    return {'id': nid(), 'type': 'ELLIPSE', 'name': name, '_x': x, '_y': y, '_w': w, '_h': h,
            'fills': paints(fill), 'strokes': paints(stroke), 'strokeWeight': sw if stroke else 0}


def vector(name, path, w, h, stroke=None, fill=None, x=0, y=0, sw=1.5):
    """A vector placeholder; its geometry is set after import (VECTOR_PATHS + vector_batch). Name must be unique
    per shape kind (all layers with the same name get the same path)."""
    VECTOR_PATHS[name] = (path, w, h)
    return {'id': nid(), 'type': 'VECTOR', 'name': name, '_x': x, '_y': y, '_w': w, '_h': h,
            'fills': paints(fill), 'strokes': paints(stroke), 'strokeWeight': sw if stroke else 0}


def F(name, w, h, kids=(), fill=None, x=0, y=0, layout=None, gap=0, pad=0, main='MIN', cross='MIN', radius=0,
      stroke=None, hug=None, clip=True, shadow=False):
    """Frame. layout='HORIZONTAL'|'VERTICAL' turns on auto layout; main/cross: MIN|CENTER|MAX|SPACE_BETWEEN;
    pad: n or (t, r, b, l); hug: 'main'|'cross'|'both' hugs that axis."""
    pad = (pad,) * 4 if isinstance(pad, (int, float)) else pad
    n = {'id': nid(), 'type': 'FRAME', 'name': name, '_x': x, '_y': y, '_w': w, '_h': h,
         'fills': paints(fill), 'strokes': paints(stroke), 'strokeWeight': 1 if stroke else 0,
         'strokeAlign': 'INSIDE', 'cornerRadius': radius, 'clipsContent': clip, 'children': list(kids)}
    if layout:
        n.update({'layoutMode': layout, 'itemSpacing': gap, 'paddingTop': pad[0], 'paddingRight': pad[1],
                  'paddingBottom': pad[2], 'paddingLeft': pad[3], 'primaryAxisAlignItems': main,
                  'counterAxisAlignItems': cross,
                  'primaryAxisSizingMode': 'AUTO' if hug in ('both', 'main') else 'FIXED',
                  'counterAxisSizingMode': 'AUTO' if hug in ('both', 'cross') else 'FIXED'})
    if shadow:
        n['effects'] = [{'type': 'DROP_SHADOW', 'color': col('#00000033'), 'offset': {'x': 0, 'y': 8},
                         'radius': 24, 'visible': True, 'blendMode': 'NORMAL'}]
    return n


def fillw(n):
    """Child stretches across its auto-layout parent (layoutSizingHorizontal FILL)."""
    n['layoutSizingHorizontal'] = 'FILL'
    return n


def fixedw(n):
    """Keep a text's width inside auto layout (otherwise a wrapped label may shrink)."""
    n['layoutSizingHorizontal'] = 'FIXED'
    return n


def at(n, x, y):
    n['_x'], n['_y'] = x, y
    return n


def walk(n):
    yield n
    for k in n.get('children', []):
        yield from walk(k)


def comp(name, node):
    """Turn a frame into a main COMPONENT registered under `name`."""
    node['type'] = 'COMPONENT'
    node['name'] = name
    COMPONENTS[name] = node
    return node


def inst(name, texts=None, x=0, y=0, w=None):
    """An INSTANCE of component `name`; texts={layer name: new characters} overrides its text layers."""
    c = COMPONENTS[name]
    n = copy.deepcopy(c)
    for m in walk(n):
        m['id'] = nid()
    n['type'] = 'INSTANCE'
    n['componentId'] = c['id']
    n['_x'], n['_y'] = x, y
    for k in walk(n):
        if k['type'] == 'TEXT' and texts and k['name'] in texts:
            k['characters'] = texts[k['name']]
            if k['style']['textAutoResize'] == 'WIDTH_AND_HEIGHT':  # fixed-width texts keep their width
                k['_w'] = est_w(k['characters'], k['style']['fontSize'], 'Mono' in k['style']['fontFamily'])
    if w:
        n['_w'] = w
    elif n.get('primaryAxisSizingMode') == 'AUTO' and n.get('layoutMode') == 'HORIZONTAL':
        n['_w'] = sum(k['_w'] for k in n['children']) + n['paddingLeft'] + n['paddingRight'] + \
            n['itemSpacing'] * (len(n['children']) - 1)
    return n


def finalize(n, ax=0.0, ay=0.0):
    """Relative _x/_y/_w/_h -> absoluteBoundingBox (recursive). Called by write_document."""
    x, y = ax + n.pop('_x', 0), ay + n.pop('_y', 0)
    w, h = n.pop('_w', 100), n.pop('_h', 100)
    n['absoluteBoundingBox'] = {'x': x, 'y': y, 'width': w, 'height': h}
    kids = n.get('children', [])
    if n.get('layoutMode'):
        horiz = n['layoutMode'] == 'HORIZONTAL'
        cur = n['paddingLeft'] if horiz else n['paddingTop']
        for k in kids:
            if k.get('layoutSizingHorizontal') == 'FILL' and not horiz:
                k['_w'] = w - n['paddingLeft'] - n['paddingRight']
            k['_x'], k['_y'] = (cur, n['paddingTop']) if horiz else (n['paddingLeft'], cur)
            cur += (k.get('_w', 100) if horiz else k.get('_h', 100)) + n['itemSpacing']
    for k in kids:
        finalize(k, x, y)
    return n


def svg_layers(path, scale=1.0, name='SVG', min_text_scale=0.35):
    """A simple SVG (rect / text / g translate / nested svg viewBox / image) as EDITABLE layers in a frame.
    Texts are skipped below min_text_scale (thumbnails stay light). Unknown elements are ignored."""
    root = ET.parse(path).getroot()
    vb = [float(v) for v in root.get('viewBox', f"0 0 {root.get('width', 100)} {root.get('height', 100)}").split()]
    kids = []

    def num(v, d=0.0):
        try:
            return float(re.sub('[^0-9.+-]', '', v)) if v not in (None, '') else d
        except ValueError:
            return d

    def hexcol(c, default):
        c = c or default
        return {'#fff': '#FFFFFF', '#000': '#000000', 'white': '#FFFFFF', 'black': '#000000'}.get(c, c)

    def emit(el, ox, oy, s, family='Georgia'):
        tag = el.tag.split('}')[-1]
        family = el.get('font-family') or family
        if tag == 'g':
            m = re.match(r'translate\(([-\d.]+)[ ,]+([-\d.]+)\)', el.get('transform', ''))
            dx, dy = (float(m.group(1)), float(m.group(2))) if m else (0, 0)
            for c in el:
                emit(c, ox + dx * s, oy + dy * s, s, family)
        elif tag == 'svg' and el is not root:
            v = [float(q) for q in el.get('viewBox', '0 0 1 1').split()]
            s2 = s * num(el.get('width'), v[2]) / v[2]
            for c in el:
                emit(c, ox + num(el.get('x')) * s, oy + num(el.get('y')) * s, s2, family)
        elif tag == 'rect':
            w, h = num(el.get('width')) * s, num(el.get('height')) * s
            if w < 1 or h < 1 or (scale < min_text_scale and (w < 6 or h < 3)):
                return
            if w >= vb[2] * scale * 0.99 and h >= vb[3] * scale * 0.99:
                return  # the page background: the frame's own fill
            fill = el.get('fill')
            fill = None if fill in (None, 'none') else hexcol(fill, '#000000')
            stroke = '#000000' if el.get('stroke') not in (None, 'none') else None
            kids.append(R(w, h, fill, stroke, ox + num(el.get('x')) * s, oy + num(el.get('y')) * s,
                          sw=max(0.5, scale), name='rect'))
        elif tag == 'text':
            txt = ''.join(el.itertext()).strip()
            if not txt or scale < min_text_scale:
                return
            size = num(el.get('font-size'), 10) * s
            w = est_w(txt, size) * 0.98
            x, y = ox + num(el.get('x')) * s, oy + num(el.get('y')) * s - size * 0.82  # baseline -> top
            anchor = el.get('text-anchor', 'start')
            x -= w / 2 if anchor == 'middle' else w if anchor == 'end' else 0
            kids.append(T(txt, round(size, 2), hexcol(el.get('fill'), '#000000'), family,
                          700 if el.get('font-weight') == 'bold' else 400, x=x, y=y, name=txt[:30],
                          lh=round(size * 1.2, 2)))
        elif tag == 'image':
            kids.append(R(num(el.get('width')) * s, num(el.get('height')) * s, '#E6E6E6', '#999999',
                          ox + num(el.get('x')) * s, oy + num(el.get('y')) * s, name='image placeholder'))
    for c in root:
        emit(c, 0, 0, scale)
    return F(name, vb[2] * scale, vb[3] * scale, kids, fill='#FFFFFF')


def page(name, frames, background='#2B2B2B'):
    return {'type': 'CANVAS', 'name': name, 'backgroundColor': col(background), 'children': list(frames)}


def write_document(out, name, pages):
    """Finalize every frame and write the REST file JSON. Returns (layers, components)."""
    for i, pg in enumerate(pages):
        pg['id'] = f'0:{i + 1}'
        for f in pg['children']:
            finalize(f)
    doc = {'name': name, 'schemaVersion': 0,
           'components': {c['id']: {'key': c['id'], 'name': c['name'], 'description': ''}
                          for c in COMPONENTS.values()},
           'document': {'id': '0:0', 'type': 'DOCUMENT', 'name': 'Document', 'children': pages}}
    with open(out, 'w', encoding='utf-8') as fh:
        json.dump(doc, fh, ensure_ascii=False)
    layers = sum(1 for pg in pages for f in pg['children'] for _ in walk(f))
    return layers, len(COMPONENTS)


def vector_batch(page_ids):
    """The MCP `batch` commands that give every recorded vector its path after import: node.find by name per page,
    then node.set {path, width, height} on the found ids ($ref chaining). Pass the page ids from open_file."""
    cmds = []
    for i, (vname, (path, w, h)) in enumerate(VECTOR_PATHS.items()):
        for j, pid in enumerate(page_ids):
            ref = f'v{i}p{j}'
            cmds.append({'command': 'node.find', 'as': ref, 'params': {'root': pid, 'name': vname, 'limit': 1000}})
            cmds.append({'command': 'node.set', 'params': {'ids': {'$ref': ref, 'field': 'ids'}, 'path': path,
                                                           'width': w, 'height': h}})
    return cmds


if __name__ == '__main__':  # self-test: python figrest.py out.json  -> then figcraft-cli convert out.json out.figcraft
    import sys
    out = sys.argv[1] if len(sys.argv) > 1 else 'figrest-selftest.json'
    comp('Button/Primary', F('Button/Primary', 96, 32, [T('Label', 13, '#FFFFFF', 'Lato', 600, name='label')],
                             fill='#15181C', layout='HORIZONTAL', pad=(0, 16, 0, 16), main='CENTER', cross='CENTER',
                             radius=6, hug='main'))
    card = F('Card', 360, 220, [T('Masuk', 22, '#15181C', 'Georgia', 600),
                                fillw(R(312, 36, '#FFFFFF', '#868D96', radius=6, name='input')),
                                inst('Button/Primary', {'label': 'Masuk'}),
                                vector('icon/doc', 'M2 1 L14 1 L20 7 L20 23 L2 23 Z', 22, 24, stroke='#15181C')],
             fill='#FFFFFF', stroke='#E3E6EA', layout='VERTICAL', gap=12, pad=24, radius=12, x=40, y=40)
    n_layers, n_comp = write_document(out, 'figrest self-test',
                                      [page('Components', [COMPONENTS['Button/Primary']]), page('Screen', [card])])
    print(f'{out}: {n_layers} layers, {n_comp} component(s), vectors to fill: {list(VECTOR_PATHS)}')
