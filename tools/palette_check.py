"""WCAG 2 contrast and colour-vision-deficiency separation (Machado 2009
simulation, CIE76 delta E) for a palette. Development only:
    python3 tools/palette_check.py '{"added":[76,176,255],"deleted":[240,72,72]}' '[33,33,33]'"""
import sys, json, itertools
def lin(c): c/=255; return c/12.92 if c<=0.04045 else ((c+0.055)/1.055)**2.4
def lum(rgb): r,g,b=map(lin,rgb); return 0.2126*r+0.7152*g+0.0722*b
def contrast(a,b): la,lb=sorted([lum(a),lum(b)],reverse=True); return (la+0.05)/(lb+0.05)
M={"deutan":[[0.367322,0.860646,-0.227968],[0.280085,0.672501,0.047413],[-0.011820,0.042940,0.968881]],
   "protan":[[0.152286,1.052583,-0.204868],[0.114503,0.786281,0.099216],[-0.003882,-0.048116,1.051998]],
   "tritan":[[1.255528,-0.076749,-0.178779],[-0.078411,0.930809,0.147602],[0.004733,0.691367,0.303900]]}
def unlin(v): v=min(max(v,0),1); return 255*(v*12.92 if v<=0.0031308 else 1.055*v**(1/2.4)-0.055)
def sim(rgb,k):
    l=[lin(c) for c in rgb]; m=M[k]; return tuple(unlin(sum(m[i][j]*l[j] for j in range(3))) for i in range(3))
def lab(rgb):
    r,g,b=map(lin,rgb); x=(0.4124*r+0.3576*g+0.1805*b)/0.95047; y=0.2126*r+0.7152*g+0.0722*b; z=(0.0193*r+0.1192*g+0.9505*b)/1.08883
    f=lambda t: t**(1/3) if t>0.008856 else 7.787*t+16/116
    return (116*f(y)-16, 500*(f(x)-f(y)), 200*(f(y)-f(z)))
def de(a,b): return sum((p-q)**2 for p,q in zip(lab(a),lab(b)))**0.5
pal=json.loads(sys.argv[1]); bg=tuple(json.loads(sys.argv[2]))
for n,c in pal.items(): print(f"{n:10} contrast on bg {contrast(c,bg):5.2f}")
for k in ["normal","deutan","protan","tritan"]:
    pairs=[]
    for (a,ca),(b,cb) in itertools.combinations(pal.items(),2):
        s=(lambda c: c) if k=="normal" else (lambda c,k=k: sim(c,k))
        pairs.append(f"{a}/{b} {de(s(ca),s(cb)):5.1f}")
    print(k, " | ".join(pairs))
