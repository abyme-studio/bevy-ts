// Stands in for a package such as @abyme/client whose types reference @types/node.
/// <reference types="node" />
export declare function connect(url: string, options?: { signal?: AbortSignal }): Promise<WebSocket>;
