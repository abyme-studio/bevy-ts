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
// Param kinds from bevy-ts, plus those installed ports declare (here bevy-ts/rapier's ReadRapierContext).
const PARAMS = new Set(["ReadRapierContext", "Query", "Res", "ResMut", "Local", "Commands", "MessageReader", "MessageWriter", "NextState", "State"]);
function decl(t: ts.TypeNode, sf: ts.SourceFile, sub: Map<string, string> = new Map()): string {
  const txt = (n: ts.Node) => sub.get(n.getText(sf)) ?? n.getText(sf);
  const at = () => `${sf.fileName.split("/").pop()}:${sf.getLineAndCharacterOfPosition(t.getStart(sf)).line + 1}`;
  if (ts.isImportTypeNode(t) && t.qualifier?.getText(sf) === "App") return "app";
  if (!ts.isTypeReferenceNode(t)) throw new Error(`${at()}: not a system param type: ${t.getText(sf)}`);
  const name = t.typeName.getText(sf), args = t.typeArguments ?? [];
  if (sub.has(name)) return sub.get(name)!;
  // An observer's trigger: On<Explode>, On<Add<Health>>.
  if (name === "On") {
    const e = args[0];
    if (ts.isTypeReferenceNode(e) && ["Add", "Insert", "Remove"].includes(e.typeName.getText(sf))) {
      return `on(${e.typeName.getText(sf)}(${txt(e.typeArguments![0])}))`;
    }
    return `on(${txt(e)})`;
  }
  // A plugin: (app: App) => void.
  if (name === "App") return "app";
  if (PARAMS.has(name)) {
    if (name === "Commands") return "commands()";
    if (name === "ReadRapierContext") return "rapier.readContext()";
    if (name !== "Query") return `${name[0].toLowerCase()}${name.slice(1)}(${args.map(txt).join(", ")})`;
    const datum = (e: ts.TypeNode): string => {
      if (!ts.isTypeReferenceNode(e)) return txt(e);
      const n = e.typeName.getText(sf);
      if (n === "Mut" || n === "Option") return `${n}(${datum(e.typeArguments![0])})`;
      return txt(e);
    };
    const data = (args[0] as ts.TupleTypeNode).elements.map(datum);
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
// Data classes: components, resources, messages and events. Field kinds come from the written type.
const DATA = new Set(["Component", "ReflectComponent", "Resource", "Message", "Event", "EntityEvent"]);
const KINDS: Record<string, string> = {
  f32: "f32", f64: "f64", i8: "i8", u8: "u8", i16: "i16", u16: "u16", i32: "i32", u32: "u32",
  bool: "bool", boolean: "bool", Entity: "entity", Vec2: "vec2", Vec3: "vec3", Vec3Like: "vec3", Vec4: "vec4", Quat: "quat",
};
function fieldKind(t: ts.TypeNode, sf: ts.SourceFile): string {
  if (ts.isUnionTypeNode(t) && t.types.every((u) => ts.isLiteralTypeNode(u) && ts.isStringLiteral(u.literal))) {
    return `enum(${t.types.map((u) => ((u as ts.LiteralTypeNode).literal as ts.StringLiteral).text).join("|")})`;
  }
  const k = KINDS[t.getText(sf)];
  if (!k) throw new Error(`${sf.fileName.split("/").pop()}:${sf.getLineAndCharacterOfPosition(t.getStart(sf)).line + 1}: ${t.getText(sf)} is not a field type (f32, u32, bool, Entity, Vec3, Quat, a string-literal union, ...)`);
  return k;
}
function readData(n: ts.ClassDeclaration, sf: ts.SourceFile, file: string) {
  const base = n.heritageClauses?.[0]?.types[0]?.expression.getText(sf) ?? "";
  if (base.startsWith("States(")) return console.log(`${n.name!.text}: states ${base.slice(6)}`);
  if (base === "SystemSet") return console.log(`${n.name!.text}: set "${file}#${n.name!.text}"`);
  if (!DATA.has(base)) return;
  let path = `${file}#${n.name!.text}`;
  const fields: string[] = [];
  try {
    for (const m of n.members) {
      if (!ts.isPropertyDeclaration(m)) continue;
      const name = m.name.getText(sf);
      if (m.modifiers?.some((x) => x.kind === ts.SyntaxKind.StaticKeyword)) {
        if (name === "path" && m.initializer && ts.isStringLiteral(m.initializer)) path = m.initializer.text;
        continue;
      }
      if (!m.type) throw new Error(`${file}: field ${name} of ${n.name!.text} needs a type annotation`);
      fields.push(`${name}: ${fieldKind(m.type, sf)}${m.initializer ? ` = ${m.initializer.getText(sf)}` : ""}`);
    }
    console.log(`${n.name!.text}: ${base.toLowerCase()} "${path}" { ${fields.join(", ")} }`);
  } catch (e) { console.log(`${n.name!.text}: ERROR ${(e as Error).message}`); }
}

const sf = load(resolve(Deno.args[0]));
const rel = Deno.args[0].replace(/^.*?examples\//, "examples/");
// Prefabs: exported arrow functions in .tsx whose one parameter is a destructured, typed props object.
// Name `module#export`, pinned by `Name.path = "..."`; props and defaults read as fields.
const pinned = new Map<string, string>();
for (const n of sf.statements) {
  if (ts.isExpressionStatement(n) && ts.isBinaryExpression(n.expression) && ts.isPropertyAccessExpression(n.expression.left)
    && n.expression.left.name.text === "path" && ts.isStringLiteral(n.expression.right)) {
    pinned.set(n.expression.left.expression.getText(sf), n.expression.right.text);
  }
}
function readPrefab(name: string, fn: ts.ArrowFunction) {
  const p = fn.parameters[0];
  if (!p || !p.type || !ts.isTypeLiteralNode(p.type) || !ts.isObjectBindingPattern(p.name)) return;
  const defaults = new Map(p.name.elements.map((e) => [e.name.getText(sf), e.initializer?.getText(sf)]));
  try {
    const props = p.type.members.map((m) => {
      const ps = m as ts.PropertySignature, k = ps.name.getText(sf), d = defaults.get(k);
      return `${k}: ${fieldKind(ps.type!, sf)}${d ? ` = ${d}` : ""}`;
    });
    console.log(`${name}: prefab "${pinned.get(name) ?? `${rel}#${name}`}" { ${props.join(", ")} }`);
  } catch (e) { console.log(`${name}: ERROR ${(e as Error).message}`); }
}
for (const n of sf.statements) {
  if (ts.isVariableStatement(n) && n.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword)) {
    for (const d of n.declarationList.declarations) {
      if (d.initializer && ts.isArrowFunction(d.initializer) && Deno.args[0].endsWith(".tsx")) readPrefab(d.name.getText(sf), d.initializer);
    }
  }
}
for (const n of sf.statements) {
  if (ts.isClassDeclaration(n) && n.name && n.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword)) readData(n, sf, rel);
  if (ts.isFunctionDeclaration(n) && n.name && n.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword)) {
    try {
      console.log(`${n.name.text}: [${n.parameters.map((p) => `${p.name.getText(sf)} = ${decl(p.type!, sf)}`).join(", ")}]`);
    } catch (e) { console.log(`${n.name.text}: ERROR ${(e as Error).message}`); }
  }
}
