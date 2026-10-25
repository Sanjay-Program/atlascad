import sys, json, math
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
fails = []
def check(name, cond, info=''):
    print(('ok  ' if cond else 'FAIL'), name, info); 
    if not cond: fails.append(name)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    for mode, url in [('server', 'http://127.0.0.1:8080/?theme=dark&lang=en'), ('static', 'http://127.0.0.1:8090/index.html?theme=dark&lang=en')]:
        ctx = b.new_context(viewport={'width': 1600, 'height': 1000}); pg = ctx.new_page()
        errors = []; pg.on('console', lambda m: errors.append(m.text) if m.type == 'error' else None); pg.on('pageerror', lambda e: errors.append(str(e)))
        pg.goto(url); pg.wait_for_timeout(2500)
        pg.wait_for_function('cadrust.status().bodies.length > 0', timeout=60000); pg.wait_for_timeout(500)
        st = pg.evaluate('cadrust.status()')
        check(f'{mode}: bracket evaluated', len(st['bodies']) == 1 and abs(st['bodies'][0]['volume'] - 21543) < 5, f"local={pg.evaluate('cadrust.local()')} vol={st['bodies'][0]['volume']:.0f}")
        check(f'{mode}: local kernel flag', pg.evaluate('cadrust.local()') == (mode == 'static'))
        names = [e['name'] for e in st['edges']]
        check(f'{mode}: every edge named', all(n and '?' not in n for n in names) and len(set(names)) == len(names), f'{len(names)} edges, e.g. {names[0]}')
        ad = pg.evaluate('cadrust.addins()')
        check(f'{mode}: add-ins listed', {a['id'] for a in ad} == {'gear-generator', 'step-batch-export'}, str(ad))
        check(f'{mode}: add-in toolbar buttons', pg.locator('#addinGroup .rrow button').count() == 2)
        # Run the gear add-in through its dialog
        pg.click('#addinGroup button >> nth=0'); pg.wait_for_timeout(300)
        check(f'{mode}: gear dialog opened', pg.locator('#dialog .df').count() == 7)
        ins = pg.locator('#dialog .df input')
        ins.nth(1).fill('18'); ins.nth(5).fill('100'); pg.click('#dOk')
        pg.wait_for_function('cadrust.status().bodies.length === 2', timeout=60000); pg.wait_for_timeout(300)
        st = pg.evaluate('cadrust.status()'); d = pg.evaluate('cadrust.doc()')
        gear = [x for x in st['bodies'] if x['id'] == 'gear1']
        check(f'{mode}: gear body built', bool(gear) and 7000 < gear[0]['volume'] < 9000 and gear[0]['bbox_min'][0] > 75, f"vol={gear[0]['volume']:.0f} bbox={gear[0]['bbox_min']}" if gear else 'none')
        check(f'{mode}: gear sketch has 18 teeth polygon', len(d['timeline'][-2]['entities'][0]['points']) == 18 * 17)
        # Undo removes the add-in step in one go
        pg.keyboard.press('Control+z'); pg.wait_for_function('cadrust.status().bodies.length === 1', timeout=60000)
        check(f'{mode}: undo reverts add-in', len(pg.evaluate('cadrust.doc()')['timeline']) == len(d['timeline']) - 2)
        # Fillet with a picked edge stores the persistent name
        pg.click('button.tool[data-tool=Fillet]'); pg.wait_for_timeout(300)
        pg.select_option('#dialog select >> nth=-1', 'pick')
        import re
        e = next(e for e in st['edges'] if e['body'] == 'base' and re.match(r'^base/e\d+\|base/end$', e['name']))
        x, y = pg.evaluate('cadrust.project(%s)' % json.dumps(e['mid'])); pg.mouse.move(x, y); pg.wait_for_timeout(100); pg.mouse.click(x, y); pg.wait_for_timeout(200)
        pg.fill('#dialog .df input >> nth=0', '3'); pg.click('#dOk')
        pg.wait_for_function('cadrust.doc().timeline.at(-1).type === "Fillet"'); pg.wait_for_timeout(1500)
        f = pg.evaluate('cadrust.doc()')['timeline'][-1]
        check(f'{mode}: pick stores name', f['edges'].get('names') == [e['name']], str(f['edges']))
        # Change the base height upstream: the pick point is stale but the name still finds the edge, no warning
        pg.evaluate("cadrust.doc().timeline.find(x => x.id === 'base').distance = 9")
        pg.click('#btnRun'); pg.wait_for_timeout(2500)
        st2 = pg.evaluate('cadrust.status()')
        fs = next(s for s in st2['status'] if s['id'] == f['id'])
        check(f'{mode}: fillet survives height change', fs['ok'] and not fs.get('warning'), str(fs))
        sel = pg.evaluate('cadrust.status().edges.filter(e => e.name.includes("|" + %s + "/f") || e.name.startsWith(%s + "/f")).length' % (json.dumps(f['id']), json.dumps(f['id'])))
        check(f'{mode}: fillet face edges named after the feature', sel >= 2, f'{sel} edges')
        if mode == 'server':
            pg.evaluate("document.querySelector('#addinGroup button:nth-of-type(1)').click()"); pg.wait_for_timeout(300)
            pg.screenshot(path=REPO + 'doc/ui_addin_gear.png'); pg.click('#dCancel')
        else:
            pg.screenshot(path=REPO + 'doc/ui_serverless.png')
        errors = [x for x in errors if not (mode == 'static' and '404' in x)]
        check(f'{mode}: console errors', not errors, '\n'.join(errors[:3]))
        ctx.close()
    b.close()
print('FAILS', fails)
