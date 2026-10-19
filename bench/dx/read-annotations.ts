// Syntax-only: read each exported function's parameter annotations, emit declarations.
import ts from "npm:typescript@5";
const src = Deno.readTextFileSync(Deno.args[0]);
const sf = ts.createSourceFile("f.ts", src, ts.ScriptTarget.Latest, true);
const text = (n: ts.Node) => n.getText(sf);
function decl(t: ts.TypeNode): string {
  if (!ts.isTypeReferenceNode(t)) return `?${text(t)}`;
  const name = text(t.typeName), args = t.typeArguments ?? [];
  if (name === "Query") {
    const data = (args[0] as ts.TupleTypeNode).elements.map((e) =>
      ts.isTypeReferenceNode(e) && text(e.typeName) === "Mut" ? `Mut(${text(e.typeArguments![0])})` : text(e));
    const f = args[1] ? (ts.isTupleTypeNode(args[1]) ? args[1].elements : [args[1]]) : [];
    const filters = f.map((x) => { const r = x as ts.TypeReferenceNode; return `.${text(r.typeName).toLowerCase()}(${text(r.typeArguments![0])})`; });
    return `query(${data.join(", ")})${filters.join("")}`;
  }
  if (name === "Res") return `res(${text(args[0])})`;
  return `?${text(t)}`;
}
sf.forEachChild((n) => {
  if (ts.isFunctionDeclaration(n) && n.name)
    console.log(`${n.name.text}: [${n.parameters.map((p) => `${text(p.name)} = ${decl(p.type!)}`).join(", ")}]`);
});
