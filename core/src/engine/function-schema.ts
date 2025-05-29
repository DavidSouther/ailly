type JSONSchemaTypeName = 'string' | 'number' | 'integer' | 'boolean' | 'object' | 'array';

interface FunctionSchemaParameters {
  type: 'object';
  properties: {
    [key: string]: {
      type: JSONSchemaTypeName;
      description?: string;
      enum?: string[];
    };
  };
  required?: string[];
  additionalProperties?: boolean;
}

interface FunctionSchema {
  name: string;
  description?: string;
  parameters: FunctionSchemaParameters;
}