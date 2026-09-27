// Wire types of reclass-server. Addresses are hex strings (they may exceed 2^53).

export type FieldType =
  | 'Hex64' | 'Hex32' | 'Hex16' | 'Hex8'
  | 'Int64' | 'Int32' | 'Int16' | 'Int8'
  | 'UInt64' | 'UInt32' | 'UInt16' | 'UInt8'
  | 'Bool' | 'Float' | 'Double'
  | 'Vector4' | 'Vector3' | 'Vector2'
  | 'Text' | 'TextPointer'
  | 'ClassInstance' | 'Pointer' | 'EncryptedPointer' | 'Enum' | 'Array'

export type Category = 'hex' | 'int' | 'float' | 'bool' | 'vec' | 'text' | 'ptr' | 'enum' | 'class' | 'array'

/** Serde's externally tagged `PointerTarget`. */
export type PointerTarget =
  | { FieldType: FieldType }
  | { ClassId: number }
  | { EnumId: number }
  | { Array: { element: PointerTarget; length: number } }
  | { Pointer: PointerTarget }

export interface ProcessEntry { pid: number; name: string }

export interface Session {
  type: 'session'
  attached: ProcessEntry | null
  live: boolean
  rootExpr: string
  rootClassId: number
  share: boolean
  projectPath: string | null
  dirty: boolean
  demo: boolean
  /** 8, or 4 for 32-bit processes. */
  pointerSize: number
}

export interface FieldDef { id: number; name: string | null; ty: FieldType; typeLabel: string; offset: number; size: number }
export interface ClassDef { id: number; name: string; size: number; refs: number; fields: FieldDef[] }
export interface EnumDef { id: number; name: string; isFlags: boolean; size: number; refs: number; variants: [string, number][] }
export interface SignatureDef {
  name: string
  module: string
  pattern: string
  offset: number
  isRelative: boolean
  relInstLen: number
  value?: string | null
  error?: string | null
}

export interface Defs { type: 'defs'; classes: ClassDef[]; enums: EnumDef[]; signatures: SignatureDef[] }

export type PortState = 'closed' | 'open' | 'stale' | 'known' | 'null'

export interface Row {
  key: string
  depth: number
  fieldId: number
  /** Owning class; null for array elements (not editable). */
  classId: number | null
  offset: number
  address: string
  size: number
  name: string | null
  ty: FieldType
  cat: Category
  typeLabel: string
  value: string
  hints: string[]
  error: string | null
  bytes: string
  pointer: string | null
  expandable: boolean
  open: boolean
  port: { state: PortState; encrypted: boolean } | null
  pointerTarget: PointerTarget | null
  enumId: number | null
  embeddedClass: number | null
  arrayLength: number | null
  arrayElement: PointerTarget | null
}

export interface LinkView {
  card: number
  key: string
  anchor: boolean
  ok: boolean
  label: string
  encrypted: boolean
  now: string | null
}

export interface CardView {
  id: number
  classId: number
  className: string
  size: number
  base: string | null
  error: string | null
  via: string | null
  encrypted: boolean
  isRoot: boolean
  x: number
  y: number
  dupOf: number | null
  links: LinkView[]
  rows: Row[]
}

export interface Frame { type: 'frame'; seq: number; rootAddress: string | null; rootError: string | null; cards: CardView[] }

/** Reply of `scan` (first scan) and `scanNext`. */
export interface ScanReply { count: number; ms: number; regions?: number; bytes?: number }

export interface ScanHit {
  address: string
  /** Current value, or null if the address is no longer readable. */
  value: string | null
  /** Value at the last scan. */
  previous: string
  /** `module+0x…` when the address is inside a module. */
  symbol: string | null
}

export interface ScanResults { count: number; type: string; results: ScanHit[] }

export const TYPE_GROUPS: [string, FieldType[]][] = [
  ['Hex', ['Hex64', 'Hex32', 'Hex16', 'Hex8']],
  ['Signed', ['Int64', 'Int32', 'Int16', 'Int8']],
  ['Unsigned', ['UInt64', 'UInt32', 'UInt16', 'UInt8']],
  ['Float', ['Float', 'Double', 'Bool']],
  ['Vector', ['Vector2', 'Vector3', 'Vector4']],
  ['Text', ['Text', 'TextPointer']],
  ['Reference', ['Pointer', 'EncryptedPointer', 'ClassInstance', 'Array', 'Enum']],
]

/** Byte sizes of fixed-size types; pointer types take the session's pointer size. */
export const TYPE_SIZE: Partial<Record<FieldType, number>> = {
  Hex64: 8, Hex32: 4, Hex16: 2, Hex8: 1, Int64: 8, Int32: 4, Int16: 2, Int8: 1, UInt64: 8, UInt32: 4, UInt16: 2, UInt8: 1,
  Bool: 1, Float: 4, Double: 8, Vector2: 8, Vector3: 12, Vector4: 16, Text: 32, TextPointer: 8, Pointer: 8, EncryptedPointer: 8, Enum: 4,
}

export function typeSize(ty: FieldType, pointerSize = 8): number | undefined {
  return ty === 'Pointer' || ty === 'EncryptedPointer' || ty === 'TextPointer' ? pointerSize : TYPE_SIZE[ty]
}

export const SHORT: Record<FieldType, string> = {
  Hex64: 'hex64', Hex32: 'hex32', Hex16: 'hex16', Hex8: 'hex8', Int64: 'i64', Int32: 'i32', Int16: 'i16', Int8: 'i8',
  UInt64: 'u64', UInt32: 'u32', UInt16: 'u16', UInt8: 'u8', Bool: 'bool', Float: 'f32', Double: 'f64',
  Vector2: 'vec2', Vector3: 'vec3', Vector4: 'vec4', Text: 'char[32]', TextPointer: 'char*', Pointer: 'ptr',
  EncryptedPointer: 'eptr', Enum: 'enum', Array: 'array', ClassInstance: 'class',
}
