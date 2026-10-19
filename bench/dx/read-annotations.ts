// Syntax-only reader for Rust-style system signatures. No type checker: it follows type aliases,
// param-bundle interfaces and relative imports by name, and fails loudly on anything else.
// deno run -A bench/dx/read-annotations.ts <file.ts>
import ts from "npm:typescript@5";
import { dirname, resolve } from "jsr:@std/path@1";

const files = new Map<string, ts.SourceFile>();
function load(path: string): ts.SourceFile {
  if (!files.has(path)) files.set(path, ts.createSourceFile(path, Deno.readTextFileSync(path), ts.ScriptTarget.Latest, true));
  return files.get(path)!;
}
type Named = ts.TypeAliasDeclaration | ts.InterfaceDeclaration;
function lookup(sf: ts.SourceFile, name: string): [Named, ts.SourceFile] | undefined {
  for (const s of sf.statements) {
    if ((ts.isTypeAliasDeclaration(s) || ts.isInterfaceDeclaration(s)) && s.name.text === name) return [s, sf];
    if (ts.isImportDeclaration(s) && s.importClause?.namedBindings && ts.isNamedImports(s.importClause.namedBindings)) {
      const spec = (s.moduleSpecifier as ts.StringLiteral).text;
      for (const el of s.importClause.namedBindings.elements) {
        if (el.name.text === name && spec.startsWith(".")) {
          return lookup(load(resolve(dirname(sf.fileName), spec)), (el.propertyName ?? el.name).text);
        }
      }
    }
  }
}
const PARAMS = new Set(["Query", "Res", "ResMut", "Local", "Commands", "MessageReader", "MessageWriter"]);
function decl(t: ts.TypeNode, sf: ts.SourceFile, sub: Map<string, string> = new Map()): string {
  const txt = (n: ts.Node) => sub.get(n.getText(sf)) ?? n.getText(sf);
  const at = () => `${sf.fileName.split("/").pop()}:${sf.getLineAndCharacterOfPosition(t.getStart(sf)).line + 1}`;
  if (!ts.isTypeReferenceNode(t)) throw new Error(`${at()}: not a system param type: ${t.getText(sf)}`);
  const name = t.typeName.getText(sf), args = t.typeArguments ?? [];
  if (sub.has(name)) return sub.get(name)!;
  if (PARAMS.has(name)) {
    if (name !== "Query") return `${name[0].toLowerCase()}${name.slice(1)}(${args.map(txt).join(", ")})`;
    const data = (args[0] as ts.TupleTypeNode).elements.map((e) =>
      ts.isTypeReferenceNode(e) && e.typeName.getText(sf) === "Mut" ? `Mut(${txt(e.typeArguments![0])})` : txt(e));
    const f = args[1] ? (ts.isTupleTypeNode(args[1]) ? args[1].elements : [args[1]]) : [];
    return `query(${data.join(", ")})` + f.map((x) => {
      const r = x as ts.TypeReferenceNode;
      return `.${r.typeName.getText(sf).toLowerCase()}(${txt(r.typeArguments![0])})`;
    }).join("");
  }
  const found = lookup(sf, name);
  if (!found) throw new Error(`${at()}: ${name} is not a system param type, an alias of one, or a param bundle`);
  const [n, nsf] = found;
  if (ts.isTypeAliasDeclaration(n)) {
    // A generic alias is text substitution: T -> the argument as written at the use site.
    const inner = new Map(sub);
    (n.typeParameters ?? []).forEach((tp, i) => {
      if (!args[i]) throw new Error(`${at()}: ${name} needs ${n.typeParameters!.length} type arguments`);
      inner.set(tp.name.text, txt(args[i]));
    });
    return decl(n.type, nsf, inner);
  }
  return `{ ${n.members.map((m) => `${(m as ts.PropertySignature).name.getText(nsf)}: ${decl((m as ts.PropertySignature).type!, nsf)}`).join(", ")} }`;
}
const sf = load(resolve(Deno.args[0]));
for (const n of sf.statements) {
  if (ts.isFunctionDeclaration(n) && n.name && n.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword)) {
    try {
      console.log(`${n.name.text}: [${n.parameters.map((p) => `${p.name.getText(sf)} = ${decl(p.type!, sf)}`).join(", ")}]`);
    } catch (e) { console.log(`${n.name.text}: ERROR ${(e as Error).message}`); }
  }
}
