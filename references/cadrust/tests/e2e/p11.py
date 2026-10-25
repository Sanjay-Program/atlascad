"""Save As / Export dialog: server folder browsing with new folders, silent re-save, export to a folder, download fallback, Data Panel folders."""
import sys, json, os, shutil, time
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
ROOT = REPO
OUT, EX = ROOT + 'doc/', ROOT + 'examples/'
BASE = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8080/'
errors, fails = [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info)
    if not cond: fails.append(name)
shutil.rmtree(EX + 'p11tmp', ignore_errors=True)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' and '404' not in m.text and '400' not in m.text else None)
    pg.goto(BASE + '?theme=dark&lang=en'); pg.wait_for_timeout(3500)
    serverless = 'index.html' in BASE
    loc = pg.evaluate('cadrust.location()')
    check('served design knows its location', loc and loc['kind'] == ('browser' if serverless else 'server'), json.dumps(loc))
    # File > Save As… opens the dialog
    pg.click('#btnFile'); pg.click('#fileMenu [data-act=saveas]'); pg.wait_for_timeout(400)
    check('save-as dialog opens', pg.is_visible('#dialog') and 'SAVE AS' in pg.inner_text('#dialog .dh').upper() and pg.input_value('#svName') == 'bracket')
    if not serverless:
        check('server folder browser shows the examples', pg.is_visible('#svBrowse') and pg.locator('#svList .ex.file').count() >= 3)
        # new folder, enter it, save under a new name
        pg.click('#svNewDir'); pg.fill('#svDirName', 'p11tmp'); pg.click('#svDirOk'); pg.wait_for_timeout(500)
        check('new folder created and entered', 'p11tmp' in pg.inner_text('#svCrumb') and os.path.isdir(EX + 'p11tmp'), pg.inner_text('#svCrumb'))
        pg.fill('#svName', 'saveas_test'); pg.click('#dOk'); pg.wait_for_timeout(600)
        path = EX + 'p11tmp/saveas_test.cadrust.json'
        check('saved into the chosen folder', os.path.isfile(path), path)
        check('message names the path', 'p11tmp/saveas_test.cadrust.json' in pg.inner_text('#msg'), pg.inner_text('#msg'))
        loc = pg.evaluate('cadrust.location()')
        check('design now lives at the new path', loc == {'kind': 'server', 'path': 'p11tmp/saveas_test.cadrust.json'}, json.dumps(loc))
        check('design tab renamed', pg.inner_text('#docName') == 'saveas_test')
        # Ctrl+S saves silently to the same place
        m0 = os.path.getmtime(path); time.sleep(1.1)
        pg.keyboard.press('Control+s'); pg.wait_for_timeout(600)
        check('ctrl+s re-saves silently', not pg.is_visible('#dialog') and os.path.getmtime(path) > m0 and 'p11tmp/saveas_test' in pg.inner_text('#msg'))
        check('served document untouched', json.load(open(EX + 'bracket.cadrust.json'))['timeline'][0]['id'] == 'base_sk' and pg.evaluate("fetch('/api/doc').then(r => r.json()).then(d => d.name)") == 'bracket')
        # export STEP into the same folder through the dialog
        pg.click('#btnFile'); pg.click('#fileMenu [data-act=exportdlg]'); pg.wait_for_timeout(400)
        check('export dialog', 'EXPORT' in pg.inner_text('#dialog .dh').upper() and pg.is_visible('#svFmt'))
        pg.select_option('#svFmt', 'step'); pg.fill('#svName', 'bracket_export'); pg.click('#dOk'); pg.wait_for_timeout(1500)
        step = EX + 'p11tmp/bracket_export.step'
        check('STEP exported into the folder', os.path.isfile(step) and os.path.getsize(step) > 5000, step)
        pg.screenshot(path=OUT + 'ui_save_as.png')
        # Data Panel browses folders and opens the saved design
        pg.click('#btnData'); pg.wait_for_timeout(500)
        check('data panel lists folders', pg.locator('#dpList .ex.dir >> text=p11tmp').count() == 1)
        pg.click('#dpList .ex.dir >> text=p11tmp'); pg.wait_for_timeout(500)
        check('folder shows the saved design', pg.locator('#dpList .ex:has-text("saveas_test")').count() == 1 and 'p11tmp' in pg.inner_text('#dpList .crumb'))
        pg.click('#dpList .ex:not(.dir) >> text=saveas_test'); pg.wait_for_timeout(2500)
        check('opened from the folder', pg.inner_text('#docName') == 'saveas_test' and pg.evaluate('cadrust.location()')['path'] == 'p11tmp/saveas_test.cadrust.json')
        # the escape hatch is closed
        code = pg.evaluate("fetch('/api/fs?path=../..').then(r => r.status)")
        check('path traversal rejected', code == 400, str(code))
        # back to bracket for the other suites
        pg.click('#btnData'); pg.wait_for_timeout(300); pg.click('#dpList .crumb a >> nth=0'); pg.wait_for_timeout(400); pg.click('#dpList .ex:not(.dir) >> text=bracket'); pg.wait_for_timeout(2000)
        check('back to bracket', pg.inner_text('#docName') == 'bracket')
    # "This computer" location with the download fallback (works in every browser)
    pg.click('#btnFile'); pg.click('#fileMenu [data-act=saveas]'); pg.wait_for_timeout(400)
    pg.select_option('#svLoc', 'computer'); pg.wait_for_timeout(100)
    check('this-computer option shows the download button', pg.is_visible('#svDl'))
    pg.fill('#svName', 'bracket_copy')
    with pg.expect_download() as dl:
        pg.click('#svDl')
    d = dl.value
    check('download carries the chosen name', d.suggested_filename == 'bracket_copy.cadrust.json', d.suggested_filename)
    if serverless:
        # browser storage is the default home in serverless mode
        pg.click('#btnFile'); pg.click('#fileMenu [data-act=saveas]'); pg.wait_for_timeout(300)
        check('serverless offers browser storage', pg.evaluate("[...document.querySelectorAll('#svLoc option')].map(o => o.value)") == ['computer', 'browser'])
        pg.select_option('#svLoc', 'browser'); pg.fill('#svName', 'bracket'); pg.click('#dOk'); pg.wait_for_timeout(500)
        check('saved to browser storage', 'Saved' in pg.inner_text('#msg') and pg.evaluate("JSON.parse(localStorage.getItem('cadrust.doc')).name") == 'bracket')
    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
shutil.rmtree(EX + 'p11tmp', ignore_errors=True)
print('FAILS', fails)
