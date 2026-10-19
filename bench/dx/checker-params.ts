// Param declarations from the real type checker (TypeScript 6 API; 7.1 brings the native one).
// deno run -A bench/dx/checker-params.ts <file.ts>
import ts from "npm:typescript@6";
import { resolve } from "jsr:@std/path@1";

const file = resolve(Deno.args[0]);
const t0 = performance.now();
const program = ts.createProgram([file], { strict: true, target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext, allowImportingTsExtensions: true, noEmit: true, skipLibCheck: true });
const checker = program.getTypeChecker();
const sf = program.getSourceFile(file)!;
const name = (t: ts.Type) => (t.aliasSymbol ?? t.getSymbol())?.getName() ?? checker.typeToString(t);
const args = (t: ts.Type) => t.aliasTypeArguments ?? ((t as ts.ObjectType).objectFlags & ts.ObjectFlags.Reference ? checker.getTypeArguments(t as ts.TypeReference) : []);
function datum(t: ts.Type): string {
  if (name(t) === "Mut") return `Mut(${name(args(t)[0])})`;
  return name(t);
}
function decl(t: ts.Type): string {
  const n = name(t);
  if (n === "Query") {
    const [data, filt] = args(t);
    const ds = checker.isTupleType(data) ? checker.getTypeArguments(data as ts.TypeReference).map(datum) : [datum(data)];
    const fs = !filt || filt.flags & ts.TypeFlags.Never ? [] : checker.isTupleType(filt) ? checker.getTypeArguments(filt as ts.TypeReference) : [filt];
    return `query(${ds.join(", ")})` + fs.map((f) => `.${name(f).toLowerCase()}(${name(args(f)[0])})`).join("");
  }
  if (n === "Res" || n === "ResMut") return `${n === "Res" ? "res" : "resMut"}(${name(args(t)[0])})`;
  // A bundle: any object type whose properties are params.
  return `{ ${checker.getPropertiesOfType(t).map((p) => `${p.name}: ${decl(checker.getTypeOfSymbol(p))}`).join(", ")} }`;
}
for (const s of sf.statements) {
  if (ts.isFunctionDeclaration(s) && s.name) {
    const sig = checker.getSignatureFromDeclaration(s)!;
    console.log(`${s.name.text}: [${sig.parameters.map((p) => `${p.name} = ${decl(checker.getTypeOfSymbol(p))}`).join(", ")}]`);
  }
}
console.log(`checked in ${(performance.now() - t0).toFixed(0)} ms (cold, TypeScript ${ts.version})`);
