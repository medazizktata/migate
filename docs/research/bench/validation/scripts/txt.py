import re,html,sys
# usage: txt.py file [needle] [before] [after] -> plain text window around needle
t=open(sys.argv[1],errors='replace').read()
t=re.sub(r'<script.*?</script>|<style.*?</style>','',t,flags=re.S|re.I);t=html.unescape(re.sub(r'<[^>]+>',' ',t));t=re.sub(r'\s+',' ',t)
if len(sys.argv)>2:
    i=t.find(sys.argv[2]); b=int(sys.argv[3]) if len(sys.argv)>3 else 500; a=int(sys.argv[4]) if len(sys.argv)>4 else 3000
    print(t[max(0,i-b):i+a] if i>=0 else 'NOT FOUND')
else: print(t)
