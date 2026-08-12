#!/usr/bin/env node

import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const schemaPath = path.join(repoRoot, 'docs', 'api_schema.json');
const outputPath = path.join(repoRoot, 'packages', 'core', 'src', 'generated', 'api-types.ts');
const checkOnly = process.argv.includes('--check');
const selfTestOnly = process.argv.includes('--self-test');

const supportedSchemaKeys = new Set([
  '$ref',
  '$schema',
  'additionalProperties',
  'allOf',
  'anyOf',
  'const',
  'default',
  'definitions',
  'description',
  'enum',
  'format',
  'items',
  'maximum',
  'minimum',
  'oneOf',
  'properties',
  'required',
  'title',
  'type',
]);

function fail(message) {
  throw new Error(message);
}

function assertSupportedSchema(schema, location) {
  if (typeof schema !== 'object' || schema === null || Array.isArray(schema)) {
    fail(`Expected a schema object at ${location}`);
  }

  for (const key of Object.keys(schema)) {
    if (!supportedSchemaKeys.has(key)) {
      fail(`Unsupported JSON Schema keyword ${JSON.stringify(key)} at ${location}`);
    }
  }
}

function refName(ref, location) {
  const prefix = '#/definitions/';
  if (typeof ref !== 'string' || !ref.startsWith(prefix)) {
    fail(`Only local definition references are supported at ${location}: ${String(ref)}`);
  }
  return decodeURIComponent(ref.slice(prefix.length));
}

function literal(value) {
  if (typeof value === 'string') {
    return `'${value.replaceAll('\\', '\\\\').replaceAll("'", "\\'")}'`;
  }
  return JSON.stringify(value);
}

function withoutNull(types) {
  const filtered = types.filter((type) => type !== 'null');
  return filtered.length > 0 ? filtered : types;
}

function parenthesizeForArray(type) {
  return type.includes(' | ') || type.includes(' & ') ? `(${type})` : type;
}

function typeExpression(schema, location, options = {}) {
  assertSupportedSchema(schema, location);

  if ('$ref' in schema) {
    return refName(schema.$ref, location);
  }

  if ('const' in schema) {
    return literal(schema.const);
  }

  if (Array.isArray(schema.enum)) {
    return schema.enum.map(literal).join(' | ');
  }

  for (const unionKey of ['oneOf', 'anyOf']) {
    if (Array.isArray(schema[unionKey])) {
      let members = schema[unionKey].map((member, index) =>
        typeExpression(member, `${location}.${unionKey}[${index}]`, options)
      );
      if (options.omitNull) {
        members = withoutNull(members);
      }
      return [...new Set(members)].join(' | ');
    }
  }

  if (Array.isArray(schema.allOf)) {
    const members = schema.allOf.map((member, index) =>
      typeExpression(member, `${location}.allOf[${index}]`, options)
    );
    return [...new Set(members)].join(' & ');
  }

  let schemaTypes = Array.isArray(schema.type) ? schema.type : [schema.type];
  schemaTypes = schemaTypes.filter((type) => type !== undefined);
  if (options.omitNull) {
    schemaTypes = withoutNull(schemaTypes);
  }

  const members = schemaTypes.map((type) => {
    switch (type) {
      case 'array': {
        if (!schema.items) fail(`Array schema is missing items at ${location}`);
        const itemType = typeExpression(schema.items, `${location}.items`);
        return `${parenthesizeForArray(itemType)}[]`;
      }
      case 'boolean':
        return 'boolean';
      case 'integer':
      case 'number':
        return 'number';
      case 'null':
        return 'null';
      case 'object':
        if (schema.properties) {
          return inlineObject(schema, location);
        }
        if (schema.additionalProperties === true) {
          return 'Record<string, unknown>';
        }
        if (schema.additionalProperties && typeof schema.additionalProperties === 'object') {
          return `Record<string, ${typeExpression(
            schema.additionalProperties,
            `${location}.additionalProperties`
          )}>`;
        }
        return 'Record<string, unknown>';
      case 'string':
        return 'string';
      default:
        fail(`Unsupported or missing schema type ${JSON.stringify(type)} at ${location}`);
    }
  });

  if (members.length === 0) {
    fail(`Schema has no supported type expression at ${location}`);
  }
  return [...new Set(members)].join(' | ');
}

function propertyName(name) {
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : JSON.stringify(name);
}

function docComment(description, indent = '') {
  if (!description) return '';
  const lines = String(description).replaceAll('*/', '*&#47;').split('\n');
  return `${indent}/**\n${lines.map((line) => `${indent} *${line ? ` ${line}` : ''}`).join('\n')}\n${indent} */\n`;
}

function runSelfTest() {
  const comment = docComment('Before */ after');
  const terminators = [...comment.matchAll(/\*\//g)];
  if (!comment.includes('Before *&#47; after') || terminators.length !== 1) {
    fail(`JSDoc terminator escaping failed: ${comment}`);
  }
  console.log('Generator self-test passed');
}

function objectMembers(schema, location, indent) {
  assertSupportedSchema(schema, location);
  const required = new Set(schema.required ?? []);
  const properties = schema.properties ?? {};
  const lines = [];

  for (const [name, propertySchema] of Object.entries(properties)) {
    const propertyLocation = `${location}.properties.${name}`;
    const optional = !required.has(name);
    lines.push(docComment(propertySchema.description, indent).trimEnd());
    lines.push(
      `${indent}${propertyName(name)}${optional ? '?' : ''}: ${typeExpression(
        propertySchema,
        propertyLocation,
        { omitNull: optional }
      )};`
    );
  }

  if (schema.additionalProperties === true) {
    lines.push(`${indent}[key: string]: unknown;`);
  } else if (schema.additionalProperties && typeof schema.additionalProperties === 'object') {
    lines.push(
      `${indent}[key: string]: ${typeExpression(
        schema.additionalProperties,
        `${location}.additionalProperties`
      )};`
    );
  }

  return lines.filter(Boolean).join('\n');
}

function inlineObject(schema, location) {
  const members = objectMembers(schema, location, '  ');
  return members ? `{\n${members}\n}` : 'Record<string, never>';
}

function declaration(name, schema) {
  const location = `definitions.${name}`;
  assertSupportedSchema(schema, location);
  const docs = docComment(schema.description);
  const isObject = schema.type === 'object' && schema.properties && !Array.isArray(schema.type);

  if (isObject) {
    const members = objectMembers(schema, location, '  ');
    return `${docs}export interface ${name} {\n${members}\n}`;
  }

  const expression = typeExpression(schema, location);
  const prefix = `export type ${name} = `;
  if (prefix.length + expression.length > 100 && expression.includes(' | ')) {
    const union = expression
      .split(' | ')
      .map((member) => `  | ${member}`)
      .join('\n');
    return `${docs}export type ${name} =\n${union};`;
  }
  return `${docs}${prefix}${expression};`;
}

function collectDefinitions(snapshot) {
  const definitions = new Map();

  for (const [rootName, rootSchema] of Object.entries(snapshot)) {
    assertSupportedSchema(rootSchema, rootName);
    const { definitions: nestedDefinitions = {}, ...rootContract } = rootSchema;
    definitions.set(rootName, rootContract);

    for (const [name, schema] of Object.entries(nestedDefinitions)) {
      const existing = definitions.get(name);
      if (existing && JSON.stringify(existing) !== JSON.stringify(schema)) {
        fail(`Conflicting definitions named ${name} in ${schemaPath}`);
      }
      if (!existing) definitions.set(name, schema);
    }
  }

  for (const [name, schema] of definitions) {
    const serialized = JSON.stringify(schema);
    for (const match of serialized.matchAll(/"\$ref":"#\/definitions\/([^"/]+)"/g)) {
      const target = decodeURIComponent(match[1]);
      if (!definitions.has(target)) fail(`Definition ${name} references missing ${target}`);
    }
  }

  return definitions;
}

function render(snapshot) {
  const definitions = collectDefinitions(snapshot);
  const header = `/**
 * GENERATED FILE — DO NOT EDIT.
 *
 * Source: docs/api_schema.json (generated from the authoritative Rust API types).
 * Regenerate with: just generate-ts-types
 *
 * Rust Option<T> fields are represented as optional TypeScript properties. Their
 * redundant JSON Schema null branch is omitted to preserve the public TS API's
 * ergonomic prop?: T surface; required nullable fields retain | null.
 */`;
  const body = [...definitions].map(([name, schema]) => declaration(name, schema)).join('\n\n');
  return `${header}\n\n${body}\n`;
}

async function main() {
  if (selfTestOnly) {
    runSelfTest();
    return;
  }

  const snapshot = JSON.parse(await readFile(schemaPath, 'utf8'));
  const generated = render(snapshot);

  if (checkOnly) {
    let committed;
    try {
      committed = await readFile(outputPath, 'utf8');
    } catch (error) {
      if (error && error.code === 'ENOENT') {
        fail(`Missing generated file ${path.relative(repoRoot, outputPath)}`);
      }
      throw error;
    }

    if (committed !== generated) {
      fail(
        `${path.relative(repoRoot, outputPath)} is out of date. Run ` +
          '`just generate-ts-types` and commit the result.'
      );
    }
    console.log(`Checked ${path.relative(repoRoot, outputPath)}`);
    return;
  }

  await writeFile(outputPath, generated, 'utf8');
  console.log(`Updated ${path.relative(repoRoot, outputPath)}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
