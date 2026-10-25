"""Face picking UX: whole-face hover highlight, one-click sketch on a face, the finished sketch is preselected for Extrude, sketch loops are selectable in the viewport."""
import os, sys, json
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
BASE = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8080/'
errors, fails = [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info)
    if not cond: fails.append(name)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' and '404' not in m.text else None)
    pg.goto(BASE + '?theme=dark&lang=en'); pg.wait_for_timeout(3500)
    # a fresh box so the faces are known: 20 mm cube at the origin
    pg.evaluate('d => cadrust.loadDoc(d, "faces")', {'name': 'faces', 'units': 'mm', 'parameters': {}, 'timeline': [{'type': 'Box', 'id': 'cube', 'size': [20, 20, 20]}]}); pg.wait_for_timeout(2500)
    pg.evaluate("cadrust.view('iso')"); pg.wait_for_timeout(300)
    top = pg.evaluate('cadrust.screen(10, 10, 20)'); side = pg.evaluate('cadrust.screen(20, 10, 10)')
    # hovering a face highlights the whole face (two triangles of the cube's top)
    pg.mouse.move(top[0], top[1]); pg.wait_for_timeout(200)
    hl = pg.evaluate('cadrust.faceHl()')
    check('hover highlights the whole top face', hl['body'] == 'cube' and hl['tris'] == 2, json.dumps(hl))
    pg.mouse.move(side[0], side[1]); pg.wait_for_timeout(200)
    check('highlight follows to the side face', pg.evaluate('cadrust.faceHl()')['tris'] == 2)
    pg.mouse.move(100, 900); pg.wait_for_timeout(200)
    check('highlight clears off the model', pg.evaluate('cadrust.faceHl()')['tris'] == 0)
    # Create Sketch: face picking is live at once; one click on the +x face starts the sketch there
    pg.click('button.tool[data-tool=Sketch]'); pg.wait_for_timeout(300)
    check('sketch start arms face picking immediately', pg.evaluate('cadrust.pickMode()'))
    pg.mouse.move(side[0], side[1]); pg.wait_for_timeout(150); pg.mouse.click(side[0], side[1]); pg.wait_for_timeout(2500)
    sk = pg.evaluate('cadrust.sketch()')
    check('one click starts the sketch on the picked face', sk['active'] and isinstance(sk['feat']['plane'], dict) and sk['feat']['plane']['face']['body'] == 'cube', json.dumps(sk['feat']['plane']))
    info = pg.evaluate("cadrust.status().sketches.find(s => s.id === cadrust.sketch().feat.id)")
    check('sketch plane is the +x face', info and abs(info['normal'][0] - 1) < 1e-6 and abs(info['origin'][0] - 20) < 1e-6, json.dumps(info and {'n': info['normal'], 'o': info['origin']}))
    # draw a circle on it and finish
    pg.click('.stool[data-stool=circle]'); pg.wait_for_timeout(100)
    for (u, v) in [(0, 0), (4, 0)]:
        x, y = pg.evaluate('cadrust.sketchUV(%s,%s)' % (u, v)); pg.mouse.move(x, y); pg.wait_for_timeout(60); pg.mouse.click(x, y); pg.wait_for_timeout(200)
    check('circle drawn', [e['type'] for e in pg.evaluate('cadrust.sketch().feat.entities')] == ['Circle'])
    pg.click('#btnFinishSketch'); pg.wait_for_timeout(1500)
    skid = pg.evaluate('cadrust.doc().timeline.at(-1).id')
    check('finished sketch is the selected sketch', pg.evaluate('cadrust.selectedSketch()') == skid)
    # E: the Extrude dialog defaults to that sketch; extruding 5 mm joins a boss on the +x face
    pg.keyboard.press('e'); pg.wait_for_timeout(300)
    check('extrude defaults to the finished sketch', pg.input_value('#dialog [data-k=sketch] select') == skid, pg.input_value('#dialog [data-k=sketch] select'))
    pg.select_option('#dialog [data-k=op] select', 'Join'); pg.fill('#dialog [data-k=distance] input', '5'); pg.click('#dOk'); pg.wait_for_timeout(2000)
    bb = pg.evaluate('cadrust.status().bodies[0]')
    check('boss extruded outward from the face', len(pg.evaluate('cadrust.status().bodies')) == 1 and abs(bb['bbox_max'][0] - 25) < 0.2 and not [s for s in pg.evaluate('cadrust.status().status') if not s['ok']], str(bb['bbox_max']))
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)
    # clicking a sketch loop in the viewport selects that sketch
    pg.evaluate('cadrust.select(null, null)'); pg.evaluate("(() => { window.__s = null; })()")
    pg.click('#tree li.d2:has-text("' + skid + '")'); pg.wait_for_timeout(100)  # toggles off (it was selected)
    check('tree click toggles the sketch selection off', pg.evaluate('cadrust.selectedSketch()') is None)
    rim = pg.evaluate('cadrust.screen(20, 14, 10)')
    pg.mouse.click(rim[0], rim[1]); pg.wait_for_timeout(300)
    check('clicking the loop in the viewport selects the sketch', pg.evaluate('cadrust.selectedSketch()') == skid, str(pg.evaluate('cadrust.selectedSketch()')))
    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
print('FAILS', fails)
