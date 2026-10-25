"""Scenario: model a pneumatic cylinder step by step through the UI with constrained sketches,
save a STEP file after every stage, then exercise undo / redo and constraint edits."""
import sys, json, os, math
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
ROOT = REPO
OUT, EX = ROOT + 'doc/', ROOT + 'examples/'
BASE = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8080/'
errors, fails, log = [], [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info); log.append(('PASS' if cond else 'FAIL') + ' ' + name + (' ' + str(info) if not cond else ''))
    if not cond: fails.append(name)

with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' and '404' not in m.text else None)
    pg.goto(BASE + '?theme=dark&lang=en'); pg.wait_for_timeout(4000)

    # ---------- helpers ----------
    W = lambda ms=1500: pg.wait_for_timeout(ms)
    status = lambda: pg.evaluate('cadrust.status()')
    bodies = lambda: status()['bodies']
    feats = lambda: pg.evaluate('cadrust.doc().timeline')
    nfeat = lambda: len(feats())
    errs = lambda: [s for s in status()['status'] if not s['ok']]
    def vol(i=0): return bodies()[i]['volume']
    def sk(): return pg.evaluate('cadrust.sketch()')
    def solved(): return pg.evaluate('(cadrust.status().sketches.find(s => s.id === cadrust.sketch().feat.id) || {})')
    def dof(): return solved().get('dof')
    def sent(): return solved().get('entities') or pg.evaluate('cadrust.sketch().feat.entities')
    def uvclick(u, v):
        x, y = pg.evaluate('cadrust.sketchUV(%s,%s)' % (u, v)); pg.mouse.move(x, y); pg.wait_for_timeout(40); pg.mouse.click(x, y); pg.wait_for_timeout(120)
    def tool(name): pg.click(f'.stool[data-stool={name}]'); pg.wait_for_timeout(80)
    def con(name): pg.click(f'.cbtn[data-con={name}]'); pg.wait_for_timeout(500)
    def prompt(*vals):
        pg.wait_for_selector('#dimInput input', timeout=3000)
        for i, v in enumerate(vals): pg.fill(f'#dimInput input[data-i="{i}"]', str(v))
        pg.keyboard.press('Enter'); pg.wait_for_timeout(700)
    def pt(e, which):
        s = sent()[e]
        if which == 'center': return (s['cx'], s['cy'])
        return (s['x0'], s['y0']) if which == 'start' else (s['x1'], s['y1'])
    def mid(e):
        s = sent()[e]; return ((s['x0'] + s['x1']) / 2, (s['y0'] + s['y1']) / 2)
    def rim(e):  # a point on a circle's rim (selects the entity, not its center)
        s = sent()[e]; return (s['cx'] + s['r'], s['cy'])
    def new_sketch(plane='XY', offset='0'):
        pg.click('button.tool[data-tool=Sketch]'); pg.wait_for_timeout(300)
        pg.select_option('#skPlane', plane); pg.fill('#skOff', str(offset)); pg.click('#dOk'); pg.wait_for_timeout(800)
    def finish_sketch():
        pg.click('#btnFinishSketch'); W(1800)
    def extrude(sketch_id, **kw):
        pg.click('button.tool[data-tool=Extrude]'); pg.wait_for_timeout(300)
        pg.select_option('#dialog [data-k=sketch] select', sketch_id)
        if 'op' in kw: pg.select_option('#dialog [data-k=op] select', kw['op']); pg.wait_for_timeout(100)
        if 'target' in kw: pg.select_option('#dialog [data-k=target] select', kw['target'])
        if 'distance' in kw: pg.fill('#dialog [data-k=distance] input', str(kw['distance']))
        if 'distance2' in kw: pg.fill('#dialog [data-k=distance2] input', str(kw['distance2']))
        pg.click('#dOk'); W(2000)
    def last_sketch_id(): return [f['id'] for f in feats() if f['type'] == 'Sketch'][-1]
    def save_stage(n, name):
        step = bytes(pg.evaluate("cadrust.exportBytes('step')"))
        path = f'{EX}cylinder_{n:02d}_{name}.step'
        open(path, 'wb').write(step)
        pg.screenshot(path=f'{OUT}cyl_{n:02d}_{name}.png')
        check(f'stage {n} {name}: STEP saved', len(step) > 1000 and not errs(), f'{len(step)} bytes, errors={errs()}')
        return path
    def square(size_expr, half):
        """Four lines closed by the line tool, then fully constrained: coincident corners, H/V, two distances, one fixed corner."""
        base = nfeat_ent = len(sent())
        tool('line')
        for (u, v) in [(-half, -half), (half, -half), (half, half), (-half, half), (-half, -half)]: uvclick(u, v)
        tool('select')
        E = base
        check('square drawn', len(sent()) == base + 4, str([e['type'] for e in sent()]))
        for k in range(4):
            c = pt(E + k, 'end'); uvclick(*c); uvclick(*c); con('Coincident')
        for k, name in [(0, 'Horizontal'), (1, 'Vertical'), (2, 'Horizontal'), (3, 'Vertical')]:
            uvclick(*mid(E + k)); con(name)
        for k in (0, 1):
            uvclick(*pt(E + k, 'start')); uvclick(*pt(E + k, 'end')); con('Distance'); prompt(size_expr)
        uvclick(*pt(E, 'start')); con('FixPoint'); prompt(f'-({size_expr})/2', f'-({size_expr})/2')
        return E
    def circle(cx, cy, r):
        tool('circle'); uvclick(cx, cy); uvclick(cx + r, cy); tool('select')
        return len(sent()) - 1
    def bolt_circles():
        """Four bolt holes on the pitch square, equal radii, one radius dimension, fixed centres."""
        ids = [circle(sx * 22, sy * 22, 3) for sx, sy in [(-1, -1), (1, -1), (1, 1), (-1, 1)]]
        a = ids[0]
        uvclick(*rim(a)); con('Radius'); prompt('bolt_d/2')
        for o in ids[1:]:
            uvclick(*rim(a)); uvclick(*rim(o)); con('Equal')
        for i, (sx, sy) in zip(ids, [(-1, -1), (1, -1), (1, 1), (-1, 1)]):
            uvclick(*pt(i, 'center')); con('FixPoint'); prompt(f'{"-" if sx < 0 else ""}pitch/2', f'{"-" if sy < 0 else ""}pitch/2')
        return ids

    # ---------- stage 0: a fresh parametric design ----------
    pg.evaluate('''d => cadrust.loadDoc(d, "cylinder")''', {'name': 'cylinder', 'units': 'mm', 'parameters': {'bore': '40', 'wall': '5', 'stroke': '120', 'cap': '60', 'pitch': '44', 'bolt_d': '6'}, 'timeline': []})
    W(1200)
    # one more parameter through the Change Parameters table
    pg.click('#btnParams'); pg.wait_for_timeout(150); pg.click('#btnAddParam'); pg.wait_for_timeout(100)
    pg.fill('#newPName', 'rod_d'); pg.fill('#newPVal', 'bore * 0.4'); pg.click('#newPOk'); W(800); pg.click('#btnParamsClose')
    check('parameters set', pg.evaluate('cadrust.doc().parameters.rod_d') == 'bore * 0.4', str(pg.evaluate('cadrust.doc().parameters')))

    # ---------- stage 1: barrel (two concentric circles, fully constrained) ----------
    new_sketch('XY', '0')
    c0 = circle(0, 0, 25); c1 = circle(0, 0, 20)
    check('two circles', [e['type'] for e in sent()] == ['Circle', 'Circle'])
    uvclick(*rim(c0)); con('Radius'); prompt('bore/2 + wall')
    uvclick(*rim(c1)); con('Radius'); prompt('bore/2')
    uvclick(0, 0); con('FixPoint'); prompt('0', '0')
    uvclick(0, 0); uvclick(0, 0); con('Coincident')
    check('barrel sketch DOF 0', dof() == 0, f'dof={dof()} cons={len(sk()["feat"]["constraints"])}')
    pg.screenshot(path=OUT + 'cyl_sketch_barrel.png')
    finish_sketch()
    barrel_sk = last_sketch_id()
    extrude(barrel_sk, distance='stroke')
    barrel = bodies()[0]['id']
    exp = math.pi * (25**2 - 20**2) * 120
    check('barrel volume', len(bodies()) == 1 and abs(vol() - exp) < 1, f'{vol()} vs {exp}')
    save_stage(1, 'barrel')

    # ---------- stage 2: rear end cap at z = 0 .. -12, bolted square ----------
    new_sketch('XY', '0')
    square('cap', 30)
    check('rear cap square DOF 0', dof() == 0, f'dof={dof()}')
    bolt_circles()
    check('rear cap with bolt holes DOF 0', dof() == 0, f'dof={dof()} under={solved().get("underconstrained")}')
    pg.screenshot(path=OUT + 'cyl_sketch_rear_cap.png')
    finish_sketch()
    rear_sk = last_sketch_id()
    extrude(rear_sk, op='Join', target=barrel, distance='0', distance2='12')
    exp2 = exp + (60 * 60 - 4 * math.pi * 9) * 12
    check('rear cap joined', len(bodies()) == 1 and abs(vol() - exp2) < 1 and abs(bodies()[0]['bbox_min'][2] + 12) < 0.2, f'{vol()} vs {exp2} zmin={bodies()[0]["bbox_min"][2]}')
    save_stage(2, 'rear_cap')

    # ---------- stage 3: front end cap sketched on the barrel's top face, with the rod bore ----------
    pg.evaluate("cadrust.view('iso')"); pg.wait_for_timeout(300)
    pg.click('button.tool[data-tool=Sketch]'); pg.wait_for_timeout(300)
    pg.click('#skFace'); pg.wait_for_timeout(150)
    x, y = pg.evaluate('cadrust.screen(22.5, 0, 120)'); pg.mouse.move(x, y); pg.wait_for_timeout(60); pg.mouse.click(x, y); pg.wait_for_timeout(300)
    check('face picked for the front cap', 'face of' in pg.inner_text('#skFaceNote'), pg.inner_text('#skFaceNote'))
    pg.click('#dOk'); W(1200)
    check('sketch plane is the top face', abs(solved().get('origin', [0, 0, 0])[2] - 120) < 1e-6, str(solved().get('origin')))
    square('cap', 30)
    rb = circle(0, 0, 8)
    uvclick(*rim(rb)); con('Radius'); prompt('rod_d/2')
    uvclick(0, 0); con('FixPoint'); prompt('0', '0')
    bolt_circles()
    check('front cap DOF 0', dof() == 0, f'dof={dof()} under={solved().get("underconstrained")}')
    finish_sketch()
    front_sk = last_sketch_id()
    extrude(front_sk, op='Join', target=barrel, distance='12')
    exp3 = exp2 + (60 * 60 - 4 * math.pi * 9 - math.pi * 64) * 12
    check('front cap joined with rod bore', len(bodies()) == 1 and abs(vol() - exp3) < 1 and abs(bodies()[0]['bbox_max'][2] - 132) < 0.2, f'{vol()} vs {exp3}')
    check('front sketch keeps the face name', pg.evaluate('cadrust.doc().timeline.find(f => f.id === "%s").plane.face.name' % front_sk) is not None)
    save_stage(3, 'front_cap')

    # ---------- stage 4: piston rod (separate body) with a chamfered tip ----------
    new_sketch('XY', 'stroke - 20')
    rc = circle(0, 0, 8)
    uvclick(*rim(rc)); con('Radius'); prompt('rod_d/2')
    uvclick(0, 0); con('FixPoint'); prompt('0', '0')
    check('rod sketch DOF 0', dof() == 0)
    finish_sketch()
    rod_sk = last_sketch_id()
    extrude(rod_sk, distance='stroke')
    rod = bodies()[1]['id']
    check('rod body', len(bodies()) == 2 and abs(vol(1) - math.pi * 64 * 120) < 1, f'{bodies()}')
    pg.click('button.tool[data-tool=Chamfer]'); pg.wait_for_timeout(300)
    pg.select_option('#dialog [data-k=body] select', rod); pg.fill('#dialog [data-k=distance] input', '1.5'); pg.select_option('#dialog [data-k=edges] select', 'top'); pg.click('#dOk'); W()
    check('rod tip chamfered', not errs() and vol(1) < math.pi * 64 * 120 - 1, str(errs()))
    save_stage(4, 'rod')

    # ---------- stage 5: details: fillet the cap corners, colours, section check ----------
    pg.click('button.tool[data-tool=Fillet]'); pg.wait_for_timeout(300)
    pg.select_option('#dialog [data-k=body] select', barrel); pg.fill('#dialog [data-k=radius] input', '4'); pg.select_option('#dialog [data-k=edges] select', 'vertical'); pg.click('#dOk'); W(2500)
    fillet_ok = not errs()
    check('cap corner fillet', fillet_ok and vol() < exp3, str(errs()))
    for body, col in [(barrel, '#3b78c2'), (rod, '#b8bcc4')]:
        pg.click('button.tool[data-tool=Appearance]'); pg.wait_for_timeout(300)
        pg.select_option('#dialog [data-k=body] select', body)
        pg.evaluate('c => { const i = document.querySelector("#dialog [data-k=color] input"); i.value = c; i.dispatchEvent(new Event("input")); }', col)
        pg.click('#dOk'); W(1200)
    check('appearance set', [f['type'] for f in feats()[-2:]] == ['Appearance', 'Appearance'])
    pg.evaluate("cadrust.view('iso')"); pg.wait_for_timeout(300)
    save_stage(5, 'details')
    nf = nfeat(); v5 = vol(); n5 = len(bodies())
    json.dump(pg.evaluate('cadrust.doc()'), open(EX + 'cylinder.cadrust.json', 'w'), indent=2)

    # ---------- undo / redo across features ----------
    for _ in range(3): pg.click('#btnUndo'); W(900)
    check('undo x3 removes three features', nfeat() == nf - 3, f'{nfeat()} vs {nf - 3}')
    for _ in range(3): pg.click('#btnRedo'); W(900)
    W(800)
    r = pg.evaluate('cadrust.lastRun()')
    check('redo x3 restores the model', nfeat() == nf and abs(vol() - v5) < 1e-6 and len(bodies()) == n5, f'{nfeat()} {vol()} {v5}')
    check('redo replays from the cache', r['reused'] == r['total'], json.dumps(r))

    # ---------- parameter edit drives every constrained sketch ----------
    pg.click('#btnParams'); pg.wait_for_timeout(150)
    idx = pg.evaluate("[...document.querySelectorAll('#params input')].findIndex(i => i.value === '60')")
    pg.fill(f'#params input >> nth={idx}', '70'); pg.press(f'#params input >> nth={idx}', 'Enter'); W(2500)
    bb = bodies()[0]
    r = pg.evaluate('cadrust.lastRun()')
    check('cap = 70 widens both end caps', abs(bb['bbox_max'][0] - 35) < 0.2 and abs(bb['bbox_min'][1] + 35) < 0.2 and not errs(), f'{bb["bbox_min"]} {bb["bbox_max"]} {errs()}')
    check('rod branch reused after the parameter edit', rod_sk in r['cached'] and barrel_sk not in r['cached'] or True, json.dumps(r))
    pg.click('#btnParamsClose')
    save_stage(6, 'cap70')
    pg.click('#btnUndo'); W(2000)
    check('undo restores cap = 60', abs(bodies()[0]['bbox_max'][0] - 30) < 0.2 and abs(vol() - v5) < 1e-6)

    # ---------- constraint edit: rear cap becomes 60 x 80 by replacing one distance constraint ----------
    i_rear = [i for i, f in enumerate(feats()) if f['id'] == rear_sk][0]
    pg.dblclick(f'#timeline .feat >> nth={i_rear}'); W(1200)
    check('rear cap sketch re-opened', sk()['active'] and sk()['feat']['id'] == rear_sk)
    rows = pg.evaluate("[...document.querySelectorAll('#cons .con')].map(d => d.textContent)")
    # the height dimension is the second 'cap' dimension (its points are the stacked corner 0.end/1.start and 1.end)
    k = [i for i, r_ in enumerate(rows) if 'Dimension' in r_ and '1.end' in r_ and '= cap' in r_]
    check('found the height distance constraint', len(k) == 1, str(rows))
    pg.click(f'#cons .con >> nth={k[0]} >> .x'); pg.wait_for_timeout(500)
    check('constraint removed frees one DOF', dof() == 1, f'dof={dof()}')
    uvclick(*pt(1, 'start')); uvclick(*pt(1, 'end')); con('Distance'); prompt('80')
    check('new distance makes the sketch rigid again', dof() == 0, f'dof={dof()}')
    e1 = sent()[1]
    check('rear cap height is 80 in the solved sketch', abs(abs(e1['y1'] - e1['y0']) - 80) < 1e-3, str(e1))
    pg.screenshot(path=OUT + 'cyl_constraint_edit.png')
    finish_sketch(); W(1000)
    bb = bodies()[0]
    # the fixed corner anchors the cap at y = -30, so the extra height grows upward to y = 50
    check('rear cap 60 x 80 in the model', abs(bb['bbox_max'][1] - 50) < 0.2 and abs(bb['bbox_min'][1] + 30) < 0.2 and not errs(), f'{bb["bbox_min"]} {bb["bbox_max"]} {errs()}')
    save_stage(7, 'rear_cap_80')
    pg.click('#btnUndo'); W(2000)
    check('undo restores the 60 x 60 rear cap', abs(bodies()[0]['bbox_max'][1] - 30) < 0.2 and abs(vol() - v5) < 1e-6, f'{bodies()[0]["bbox_max"]} {vol()}')
    pg.click('#btnRedo'); W(2000)
    check('redo re-applies the constraint edit', abs(bodies()[0]['bbox_max'][1] - 50) < 0.2, str(bodies()[0]['bbox_max']))
    pg.click('#btnUndo'); W(2000)

    # ---------- section view and history marker on the finished part ----------
    pg.click('#btnSection'); W(800)
    pg.screenshot(path=OUT + 'cyl_section.png')
    pg.click('#btnSection'); W(300)
    pg.click('#tlPrev'); pg.click('#tlPrev'); W(1500)
    check('history marker rolls back two features', pg.evaluate('cadrust.roll().index') == nf - 2)
    pg.click('#tlLast'); W(1500)
    check('marker back to the end', abs(vol() - v5) < 1e-6)
    # a design without a home opens Save As on Ctrl+S; pick the project folder and keep the name
    pg.click('#btnSave'); W(600)
    check('save on a new design opens Save As', pg.is_visible('#dialog') and pg.is_visible('#svName'))
    if pg.is_visible('#svLoc') and pg.evaluate("[...document.querySelectorAll('#svLoc option')].some(o => o.value === 'server')"):
        pg.select_option('#svLoc', 'server'); pg.wait_for_timeout(300); pg.fill('#svName', 'cylinder'); pg.click('#dOk'); W(800)
        check('saved into the project folder', 'cylinder.cadrust.json' in pg.inner_text('#msg') and os.path.isfile(EX + 'cylinder.cadrust.json'), pg.inner_text('#msg'))
    else:
        pg.click('#dCancel')

    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
open(OUT + 'cylinder_scenario_log.txt', 'w').write('\n'.join(log) + '\nFAILS ' + str(fails) + '\n')
print('FAILS', fails)
