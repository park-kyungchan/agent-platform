/** Generated from git-work-v3.schema.json; do not edit. SHA-256: 1f1542021ee00b044bc5d0d7577430aeba58a0d2431aa92fcb7f05c1df7e27e0 */
// @bun
var __commonJS = (cb, mod) => () => (mod || cb((mod = { exports: {} }).exports, mod), mod.exports);

// node_modules/.bun/ajv@8.20.0/node_modules/ajv/dist/runtime/ucs2length.js
var require_ucs2length = __commonJS(function(exports) {
  Object.defineProperty(exports, "__esModule", { value: true });
  function ucs2length(str) {
    const len = str.length;
    let length = 0;
    let pos = 0;
    let value;
    while (pos < len) {
      length++;
      value = str.charCodeAt(pos++);
      if (value >= 55296 && value <= 56319 && pos < len) {
        value = str.charCodeAt(pos);
        if ((value & 64512) === 56320)
          pos++;
      }
    }
    return length;
  }
  exports.default = ucs2length;
  ucs2length.code = 'require("ajv/dist/runtime/ucs2length").default';
});

// packages/ontology/scripts/.git-work-v3-standalone.generated.js
var validateGraph = validate222;
var schema7 = { type: "object", properties: { schema: { const: "common.git-work.v3" }, domains: { type: "array", items: { $ref: "#/$defs/Domain" }, maxItems: 2000 }, projects: { type: "array", items: { $ref: "#/$defs/Project" }, maxItems: 2000 }, works: { type: "array", items: { $ref: "#/$defs/Work" }, maxItems: 2000 }, tasks: { type: "array", items: { $ref: "#/$defs/Task" }, maxItems: 2000 }, contracts: { type: "array", items: { $ref: "#/$defs/Contract" }, maxItems: 1e4 }, dependencies: { type: "array", items: { $ref: "#/$defs/Dependency" }, maxItems: 1e4 }, relations: { type: "array", items: { $ref: "#/$defs/Related" }, maxItems: 1e4 }, delegations: { type: "array", items: { $ref: "#/$defs/Delegation" }, maxItems: 1e4 }, provenance: { type: "array", items: { $ref: "#/$defs/Provenance" }, maxItems: 1e4 }, nativeCorrelations: { type: "array", items: { $ref: "#/$defs/NativeCorrelation" }, maxItems: 1e4 }, aliases: { type: "array", items: { $ref: "#/$defs/GitWorkAlias" }, maxItems: 2048 }, externalArtifacts: { type: "array", items: { $ref: "#/$defs/SelectedExternalArtifact" }, maxItems: 2048 } }, required: ["schema", "domains", "projects", "works", "tasks", "contracts", "dependencies", "relations", "delegations", "provenance", "nativeCorrelations"], additionalProperties: false };
var func2 = Object.prototype.hasOwnProperty;
var func3 = require_ucs2length().default;
var pattern0 = new RegExp("^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$", "u");
function validate24(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (typeof data === "string") {
      if (func3(data) > 240) {
        validate24.errors = [{ instancePath, schemaPath: "#/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
        return false;
      } else {
        if (func3(data) < 1) {
          validate24.errors = [{ instancePath, schemaPath: "#/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
          return false;
        } else {
          if (!pattern0.test(data)) {
            validate24.errors = [{ instancePath, schemaPath: "#/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
            return false;
          }
        }
      }
    } else {
      validate24.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
      return false;
    }
  }
  validate24.errors = vErrors;
  return errors === 0;
}
function validate23(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.title === undefined && (missing0 = "title")) {
        validate23.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "title" || key0 === "parentDomainId")) {
            validate23.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.title !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.title, { instancePath: instancePath + "/title", parentData: data, parentDataProperty: "title", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.parentDomainId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.parentDomainId, { instancePath: instancePath + "/parentDomainId", parentData: data, parentDataProperty: "parentDomainId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate23.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate23.errors = vErrors;
  return errors === 0;
}
function validate29(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.title === undefined && (missing0 = "title") || data.primaryDomainId === undefined && (missing0 = "primaryDomainId")) {
        validate29.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "title" || key0 === "primaryDomainId")) {
            validate29.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.title !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.title, { instancePath: instancePath + "/title", parentData: data, parentDataProperty: "title", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.primaryDomainId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.primaryDomainId, { instancePath: instancePath + "/primaryDomainId", parentData: data, parentDataProperty: "primaryDomainId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate29.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate29.errors = vErrors;
  return errors === 0;
}
var schema11 = { type: "object", properties: { projectId: { $ref: "#/$defs/Id" }, workId: { $ref: "#/$defs/Id" }, title: { $ref: "#/$defs/Id" }, leadActorId: { anyOf: [{ $ref: "#/$defs/Id" }, { type: "null" }] }, revision: { $ref: "#/$defs/Revision" }, contracts: { $ref: "#/$defs/ContractSet" }, status: { enum: ["proposed", "admitted", "active", "verified", "accepted", "cancelled"] }, legacySource: { $ref: "#/$defs/GitSourcePin" } }, required: ["projectId", "workId", "title", "leadActorId", "revision", "contracts", "status"], additionalProperties: false };
function validate39(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (!(typeof data == "number" && (!(data % 1) && !isNaN(data)) && isFinite(data))) {
    validate39.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
    return false;
  }
  if (errors === 0) {
    if (typeof data == "number" && isFinite(data)) {
      if (data > 9007199254740991 || isNaN(data)) {
        validate39.errors = [{ instancePath, schemaPath: "#/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
        return false;
      } else {
        if (data < 1 || isNaN(data)) {
          validate39.errors = [{ instancePath, schemaPath: "#/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
          return false;
        }
      }
    }
  }
  validate39.errors = vErrors;
  return errors === 0;
}
var schema14 = { type: "object", properties: { id: { $ref: "#/$defs/Id" }, owner: { $ref: "#/$defs/ContractOwner" }, revision: { $ref: "#/$defs/Revision" }, kind: { enum: ["issue", "spec", "plan"] }, digest: { $ref: "#/$defs/Digest" } }, required: ["id", "owner", "revision", "kind", "digest"], additionalProperties: false };
function validate44(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (errors === _errs1) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.kind === undefined && (missing0 = "kind") || data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId")) {
        const err0 = { instancePath, schemaPath: "#/oneOf/0/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        const _errs3 = errors;
        for (const key0 in data) {
          if (!(key0 === "kind" || key0 === "projectId" || key0 === "workId")) {
            const err1 = { instancePath, schemaPath: "#/oneOf/0/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
            break;
          }
        }
        if (_errs3 === errors) {
          if (data.kind !== undefined) {
            const _errs4 = errors;
            if (data.kind !== "work") {
              const err2 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/0/properties/kind/const", keyword: "const", params: { allowedValue: "work" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err2];
              } else {
                vErrors.push(err2);
              }
              errors++;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.projectId !== undefined) {
              const _errs5 = errors;
              if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.workId !== undefined) {
                const _errs6 = errors;
                if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid1 = _errs6 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      }
    } else {
      const err3 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err3];
      } else {
        vErrors.push(err3);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs7 = errors;
  if (errors === _errs7) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.kind === undefined && (missing1 = "kind") || data.projectId === undefined && (missing1 = "projectId") || data.workId === undefined && (missing1 = "workId") || data.taskId === undefined && (missing1 = "taskId")) {
        const err4 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err4];
        } else {
          vErrors.push(err4);
        }
        errors++;
      } else {
        const _errs9 = errors;
        for (const key1 in data) {
          if (!(key1 === "kind" || key1 === "projectId" || key1 === "workId" || key1 === "taskId")) {
            const err5 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err5];
            } else {
              vErrors.push(err5);
            }
            errors++;
            break;
          }
        }
        if (_errs9 === errors) {
          if (data.kind !== undefined) {
            const _errs10 = errors;
            if (data.kind !== "task") {
              const err6 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "task" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err6];
              } else {
                vErrors.push(err6);
              }
              errors++;
            }
            var valid2 = _errs10 === errors;
          } else {
            var valid2 = true;
          }
          if (valid2) {
            if (data.projectId !== undefined) {
              const _errs11 = errors;
              if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid2 = _errs11 === errors;
            } else {
              var valid2 = true;
            }
            if (valid2) {
              if (data.workId !== undefined) {
                const _errs12 = errors;
                if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid2 = _errs12 === errors;
              } else {
                var valid2 = true;
              }
              if (valid2) {
                if (data.taskId !== undefined) {
                  const _errs13 = errors;
                  if (!validate24(data.taskId, { instancePath: instancePath + "/taskId", parentData: data, parentDataProperty: "taskId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid2 = _errs13 === errors;
                } else {
                  var valid2 = true;
                }
              }
            }
          }
        }
      }
    } else {
      const err7 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err7];
      } else {
        vErrors.push(err7);
      }
      errors++;
    }
  }
  var _valid0 = _errs7 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
  }
  if (!valid0) {
    const err8 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err8];
    } else {
      vErrors.push(err8);
    }
    errors++;
    validate44.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate44.errors = vErrors;
  return errors === 0;
}
var pattern1 = new RegExp("^[a-f0-9]{64}$", "u");
function validate52(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (typeof data === "string") {
      if (!pattern1.test(data)) {
        validate52.errors = [{ instancePath, schemaPath: "#/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
        return false;
      }
    } else {
      validate52.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
      return false;
    }
  }
  validate52.errors = vErrors;
  return errors === 0;
}
function validate42(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.owner === undefined && (missing0 = "owner") || data.revision === undefined && (missing0 = "revision") || data.kind === undefined && (missing0 = "kind") || data.digest === undefined && (missing0 = "digest")) {
        validate42.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "owner" || key0 === "revision" || key0 === "kind" || key0 === "digest")) {
            validate42.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.owner !== undefined) {
              const _errs3 = errors;
              if (!validate44(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.revision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.kind !== undefined) {
                  let data3 = data.kind;
                  const _errs5 = errors;
                  if (!(data3 === "issue" || data3 === "spec" || data3 === "plan")) {
                    validate42.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema14.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.digest !== undefined) {
                    const _errs6 = errors;
                    if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate42.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate42.errors = vErrors;
  return errors === 0;
}
function validate41(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.issue === undefined && (missing0 = "issue") || data.spec === undefined && (missing0 = "spec") || data.plan === undefined && (missing0 = "plan")) {
        validate41.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "issue" || key0 === "spec" || key0 === "plan")) {
            validate41.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.issue !== undefined) {
            const _errs2 = errors;
            if (!validate42(data.issue, { instancePath: instancePath + "/issue", parentData: data, parentDataProperty: "issue", rootData })) {
              vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.spec !== undefined) {
              const _errs3 = errors;
              if (!validate42(data.spec, { instancePath: instancePath + "/spec", parentData: data, parentDataProperty: "spec", rootData })) {
                vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.plan !== undefined) {
                const _errs4 = errors;
                if (!validate42(data.plan, { instancePath: instancePath + "/plan", parentData: data, parentDataProperty: "plan", rootData })) {
                  vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate41.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate41.errors = vErrors;
  return errors === 0;
}
var pattern2 = new RegExp("^[a-f0-9]{40}$", "u");
function validate58(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.repositoryId === undefined && (missing0 = "repositoryId") || data.commit === undefined && (missing0 = "commit") || data.path === undefined && (missing0 = "path") || data.sha256 === undefined && (missing0 = "sha256")) {
        validate58.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "repositoryId" || key0 === "commit" || key0 === "path" || key0 === "sha256")) {
            validate58.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.repositoryId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.repositoryId, { instancePath: instancePath + "/repositoryId", parentData: data, parentDataProperty: "repositoryId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.commit !== undefined) {
              let data1 = data.commit;
              const _errs3 = errors;
              if (errors === _errs3) {
                if (typeof data1 === "string") {
                  if (!pattern2.test(data1)) {
                    validate58.errors = [{ instancePath: instancePath + "/commit", schemaPath: "#/properties/commit/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{40}$" }, message: 'must match pattern "' + "^[a-f0-9]{40}$" + '"' }];
                    return false;
                  }
                } else {
                  validate58.errors = [{ instancePath: instancePath + "/commit", schemaPath: "#/properties/commit/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.path !== undefined) {
                let data2 = data.path;
                const _errs5 = errors;
                if (errors === _errs5) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 4096) {
                      validate58.errors = [{ instancePath: instancePath + "/path", schemaPath: "#/properties/path/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate58.errors = [{ instancePath: instancePath + "/path", schemaPath: "#/properties/path/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      }
                    }
                  } else {
                    validate58.errors = [{ instancePath: instancePath + "/path", schemaPath: "#/properties/path/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.sha256 !== undefined) {
                  const _errs7 = errors;
                  if (!validate52(data.sha256, { instancePath: instancePath + "/sha256", parentData: data, parentDataProperty: "sha256", rootData })) {
                    vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate58.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate58.errors = vErrors;
  return errors === 0;
}
function validate34(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.title === undefined && (missing0 = "title") || data.leadActorId === undefined && (missing0 = "leadActorId") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts") || data.status === undefined && (missing0 = "status")) {
        validate34.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId" || key0 === "title" || key0 === "leadActorId" || key0 === "revision" || key0 === "contracts" || key0 === "status" || key0 === "legacySource")) {
            validate34.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.title !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.title, { instancePath: instancePath + "/title", parentData: data, parentDataProperty: "title", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.leadActorId !== undefined) {
                  let data3 = data.leadActorId;
                  const _errs5 = errors;
                  const _errs6 = errors;
                  let valid1 = false;
                  const _errs7 = errors;
                  if (!validate24(data3, { instancePath: instancePath + "/leadActorId", parentData: data, parentDataProperty: "leadActorId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var _valid0 = _errs7 === errors;
                  valid1 = valid1 || _valid0;
                  if (!valid1) {
                    const _errs8 = errors;
                    if (data3 !== null) {
                      const err0 = { instancePath: instancePath + "/leadActorId", schemaPath: "#/properties/leadActorId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                      if (vErrors === null) {
                        vErrors = [err0];
                      } else {
                        vErrors.push(err0);
                      }
                      errors++;
                    }
                    var _valid0 = _errs8 === errors;
                    valid1 = valid1 || _valid0;
                  }
                  if (!valid1) {
                    const err1 = { instancePath: instancePath + "/leadActorId", schemaPath: "#/properties/leadActorId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                    if (vErrors === null) {
                      vErrors = [err1];
                    } else {
                      vErrors.push(err1);
                    }
                    errors++;
                    validate34.errors = vErrors;
                    return false;
                  } else {
                    errors = _errs6;
                    if (vErrors !== null) {
                      if (_errs6) {
                        vErrors.length = _errs6;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.revision !== undefined) {
                    const _errs10 = errors;
                    if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                      vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.contracts !== undefined) {
                      const _errs11 = errors;
                      if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                        vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.status !== undefined) {
                        let data6 = data.status;
                        const _errs12 = errors;
                        if (!(data6 === "proposed" || data6 === "admitted" || data6 === "active" || data6 === "verified" || data6 === "accepted" || data6 === "cancelled")) {
                          validate34.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema11.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.legacySource !== undefined) {
                          const _errs13 = errors;
                          if (!validate58(data.legacySource, { instancePath: instancePath + "/legacySource", parentData: data, parentDataProperty: "legacySource", rootData })) {
                            vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs13 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate34.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate34.errors = vErrors;
  return errors === 0;
}
var schema18 = { type: "object", properties: { projectId: { $ref: "#/$defs/Id" }, workId: { $ref: "#/$defs/Id" }, taskId: { $ref: "#/$defs/Id" }, title: { $ref: "#/$defs/Id" }, parentTaskId: { $ref: "#/$defs/Id" }, assigneeActorId: { anyOf: [{ $ref: "#/$defs/Id" }, { type: "null" }] }, revision: { $ref: "#/$defs/Revision" }, contractOverrides: { $ref: "#/$defs/ContractOverrides" }, acceptanceCriteria: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2000 }, resources: { type: "array", items: { $ref: "#/$defs/ResourceClaim" }, maxItems: 2000 }, status: { enum: ["planned", "ready", "running", "blocked", "verified", "accepted", "cancelled"] }, legacyBinding: { $ref: "#/$defs/LegacyTaskBinding" } }, required: ["projectId", "workId", "taskId", "title", "assigneeActorId", "revision", "contractOverrides", "acceptanceCriteria", "resources", "status"], additionalProperties: false };
function validate71(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      const _errs1 = errors;
      for (const key0 in data) {
        if (!(key0 === "issue" || key0 === "spec" || key0 === "plan")) {
          validate71.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
          return false;
          break;
        }
      }
      if (_errs1 === errors) {
        if (data.issue !== undefined) {
          const _errs2 = errors;
          if (!validate42(data.issue, { instancePath: instancePath + "/issue", parentData: data, parentDataProperty: "issue", rootData })) {
            vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
            errors = vErrors.length;
          }
          var valid0 = _errs2 === errors;
        } else {
          var valid0 = true;
        }
        if (valid0) {
          if (data.spec !== undefined) {
            const _errs3 = errors;
            if (!validate42(data.spec, { instancePath: instancePath + "/spec", parentData: data, parentDataProperty: "spec", rootData })) {
              vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs3 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.plan !== undefined) {
              const _errs4 = errors;
              if (!validate42(data.plan, { instancePath: instancePath + "/plan", parentData: data, parentDataProperty: "plan", rootData })) {
                vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs4 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate71.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate71.errors = vErrors;
  return errors === 0;
}
var schema20 = { type: "object", properties: { resource: { $ref: "#/$defs/Id" }, mode: { enum: ["read", "write"] } }, required: ["resource", "mode"], additionalProperties: false };
function validate77(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.resource === undefined && (missing0 = "resource") || data.mode === undefined && (missing0 = "mode")) {
        validate77.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "resource" || key0 === "mode")) {
            validate77.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.resource !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.resource, { instancePath: instancePath + "/resource", parentData: data, parentDataProperty: "resource", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.mode !== undefined) {
              let data1 = data.mode;
              const _errs3 = errors;
              if (!(data1 === "read" || data1 === "write")) {
                validate77.errors = [{ instancePath: instancePath + "/mode", schemaPath: "#/properties/mode/enum", keyword: "enum", params: { allowedValues: schema20.properties.mode.enum }, message: "must be equal to one of the allowed values" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate77.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate77.errors = vErrors;
  return errors === 0;
}
var schema21 = { type: "object", properties: { protocol: { enum: ["common.git-work.v1", "common.git-work-task.v2"] }, source: { $ref: "#/$defs/GitSourcePin" }, contracts: { $ref: "#/$defs/ContractSet" }, holdReasons: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2048 } }, required: ["protocol", "source", "contracts", "holdReasons"], additionalProperties: false };
function validate80(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.protocol === undefined && (missing0 = "protocol") || data.source === undefined && (missing0 = "source") || data.contracts === undefined && (missing0 = "contracts") || data.holdReasons === undefined && (missing0 = "holdReasons")) {
        validate80.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "protocol" || key0 === "source" || key0 === "contracts" || key0 === "holdReasons")) {
            validate80.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.protocol !== undefined) {
            let data0 = data.protocol;
            const _errs2 = errors;
            if (!(data0 === "common.git-work.v1" || data0 === "common.git-work-task.v2")) {
              validate80.errors = [{ instancePath: instancePath + "/protocol", schemaPath: "#/properties/protocol/enum", keyword: "enum", params: { allowedValues: schema21.properties.protocol.enum }, message: "must be equal to one of the allowed values" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.source !== undefined) {
              const _errs3 = errors;
              if (!validate58(data.source, { instancePath: instancePath + "/source", parentData: data, parentDataProperty: "source", rootData })) {
                vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.contracts !== undefined) {
                const _errs4 = errors;
                if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                  vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.holdReasons !== undefined) {
                  let data3 = data.holdReasons;
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 2048) {
                        validate80.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate24(data3[i0], { instancePath: instancePath + "/holdReasons/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate80.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate80.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate80.errors = vErrors;
  return errors === 0;
}
function validate63(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.taskId === undefined && (missing0 = "taskId") || data.title === undefined && (missing0 = "title") || data.assigneeActorId === undefined && (missing0 = "assigneeActorId") || data.revision === undefined && (missing0 = "revision") || data.contractOverrides === undefined && (missing0 = "contractOverrides") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.resources === undefined && (missing0 = "resources") || data.status === undefined && (missing0 = "status")) {
        validate63.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema18.properties, key0)) {
            validate63.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.taskId, { instancePath: instancePath + "/taskId", parentData: data, parentDataProperty: "taskId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.title !== undefined) {
                  const _errs5 = errors;
                  if (!validate24(data.title, { instancePath: instancePath + "/title", parentData: data, parentDataProperty: "title", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.parentTaskId !== undefined) {
                    const _errs6 = errors;
                    if (!validate24(data.parentTaskId, { instancePath: instancePath + "/parentTaskId", parentData: data, parentDataProperty: "parentTaskId", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.assigneeActorId !== undefined) {
                      let data5 = data.assigneeActorId;
                      const _errs7 = errors;
                      const _errs8 = errors;
                      let valid1 = false;
                      const _errs9 = errors;
                      if (!validate24(data5, { instancePath: instancePath + "/assigneeActorId", parentData: data, parentDataProperty: "assigneeActorId", rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var _valid0 = _errs9 === errors;
                      valid1 = valid1 || _valid0;
                      if (!valid1) {
                        const _errs10 = errors;
                        if (data5 !== null) {
                          const err0 = { instancePath: instancePath + "/assigneeActorId", schemaPath: "#/properties/assigneeActorId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                          if (vErrors === null) {
                            vErrors = [err0];
                          } else {
                            vErrors.push(err0);
                          }
                          errors++;
                        }
                        var _valid0 = _errs10 === errors;
                        valid1 = valid1 || _valid0;
                      }
                      if (!valid1) {
                        const err1 = { instancePath: instancePath + "/assigneeActorId", schemaPath: "#/properties/assigneeActorId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                        if (vErrors === null) {
                          vErrors = [err1];
                        } else {
                          vErrors.push(err1);
                        }
                        errors++;
                        validate63.errors = vErrors;
                        return false;
                      } else {
                        errors = _errs8;
                        if (vErrors !== null) {
                          if (_errs8) {
                            vErrors.length = _errs8;
                          } else {
                            vErrors = null;
                          }
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.revision !== undefined) {
                        const _errs12 = errors;
                        if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                          vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.contractOverrides !== undefined) {
                          const _errs13 = errors;
                          if (!validate71(data.contractOverrides, { instancePath: instancePath + "/contractOverrides", parentData: data, parentDataProperty: "contractOverrides", rootData })) {
                            vErrors = vErrors === null ? validate71.errors : vErrors.concat(validate71.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs13 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.acceptanceCriteria !== undefined) {
                            let data8 = data.acceptanceCriteria;
                            const _errs14 = errors;
                            if (errors === _errs14) {
                              if (Array.isArray(data8)) {
                                if (data8.length > 2000) {
                                  validate63.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len0 = data8.length;
                                  for (let i0 = 0;i0 < len0; i0++) {
                                    const _errs16 = errors;
                                    if (!validate24(data8[i0], { instancePath: instancePath + "/acceptanceCriteria/" + i0, parentData: data8, parentDataProperty: i0, rootData })) {
                                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs16 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate63.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs14 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.resources !== undefined) {
                              let data10 = data.resources;
                              const _errs17 = errors;
                              if (errors === _errs17) {
                                if (Array.isArray(data10)) {
                                  if (data10.length > 2000) {
                                    validate63.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid3 = true;
                                    const len1 = data10.length;
                                    for (let i1 = 0;i1 < len1; i1++) {
                                      const _errs19 = errors;
                                      if (!validate77(data10[i1], { instancePath: instancePath + "/resources/" + i1, parentData: data10, parentDataProperty: i1, rootData })) {
                                        vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid3 = _errs19 === errors;
                                      if (!valid3) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate63.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs17 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.status !== undefined) {
                                let data12 = data.status;
                                const _errs20 = errors;
                                if (!(data12 === "planned" || data12 === "ready" || data12 === "running" || data12 === "blocked" || data12 === "verified" || data12 === "accepted" || data12 === "cancelled")) {
                                  validate63.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema18.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid0 = _errs20 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.legacyBinding !== undefined) {
                                  const _errs21 = errors;
                                  if (!validate80(data.legacyBinding, { instancePath: instancePath + "/legacyBinding", parentData: data, parentDataProperty: "legacyBinding", rootData })) {
                                    vErrors = vErrors === null ? validate80.errors : vErrors.concat(validate80.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid0 = _errs21 === errors;
                                } else {
                                  var valid0 = true;
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate63.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate63.errors = vErrors;
  return errors === 0;
}
function validate91(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.owner === undefined && (missing0 = "owner") || data.uri === undefined && (missing0 = "uri") || data.revision === undefined && (missing0 = "revision") || data.digest === undefined && (missing0 = "digest")) {
        validate91.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "owner" || key0 === "uri" || key0 === "revision" || key0 === "digest")) {
            validate91.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.owner !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.uri !== undefined) {
              let data1 = data.uri;
              const _errs3 = errors;
              if (errors === _errs3) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 4096) {
                    validate91.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate91.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    }
                  }
                } else {
                  validate91.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.revision !== undefined) {
                const _errs5 = errors;
                if (!validate24(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.digest !== undefined) {
                  const _errs6 = errors;
                  if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                    vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs6 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate91.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate91.errors = vErrors;
  return errors === 0;
}
var wrapper0 = { validate: validate97 };
function validate97(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  const _errs0 = errors;
  let valid0 = false;
  const _errs1 = errors;
  if (data !== null) {
    const err0 = { instancePath, schemaPath: "#/anyOf/0/type", keyword: "type", params: { type: "null" }, message: "must be null" };
    if (vErrors === null) {
      vErrors = [err0];
    } else {
      vErrors.push(err0);
    }
    errors++;
  }
  var _valid0 = _errs1 === errors;
  valid0 = valid0 || _valid0;
  if (!valid0) {
    const _errs3 = errors;
    if (typeof data !== "boolean") {
      const err1 = { instancePath, schemaPath: "#/anyOf/1/type", keyword: "type", params: { type: "boolean" }, message: "must be boolean" };
      if (vErrors === null) {
        vErrors = [err1];
      } else {
        vErrors.push(err1);
      }
      errors++;
    }
    var _valid0 = _errs3 === errors;
    valid0 = valid0 || _valid0;
    if (!valid0) {
      const _errs5 = errors;
      if (errors === _errs5) {
        if (typeof data === "string") {
          if (func3(data) > 65536) {
            const err2 = { instancePath, schemaPath: "#/anyOf/2/maxLength", keyword: "maxLength", params: { limit: 65536 }, message: "must NOT have more than 65536 characters" };
            if (vErrors === null) {
              vErrors = [err2];
            } else {
              vErrors.push(err2);
            }
            errors++;
          }
        } else {
          const err3 = { instancePath, schemaPath: "#/anyOf/2/type", keyword: "type", params: { type: "string" }, message: "must be string" };
          if (vErrors === null) {
            vErrors = [err3];
          } else {
            vErrors.push(err3);
          }
          errors++;
        }
      }
      var _valid0 = _errs5 === errors;
      valid0 = valid0 || _valid0;
      if (!valid0) {
        const _errs7 = errors;
        if (!(typeof data == "number" && (!(data % 1) && !isNaN(data)) && isFinite(data))) {
          const err4 = { instancePath, schemaPath: "#/anyOf/3/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
          if (vErrors === null) {
            vErrors = [err4];
          } else {
            vErrors.push(err4);
          }
          errors++;
        }
        if (errors === _errs7) {
          if (typeof data == "number" && isFinite(data)) {
            if (data > 9007199254740991 || isNaN(data)) {
              const err5 = { instancePath, schemaPath: "#/anyOf/3/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
              if (vErrors === null) {
                vErrors = [err5];
              } else {
                vErrors.push(err5);
              }
              errors++;
            } else {
              if (data < -9007199254740991 || isNaN(data)) {
                const err6 = { instancePath, schemaPath: "#/anyOf/3/minimum", keyword: "minimum", params: { comparison: ">=", limit: -9007199254740991 }, message: "must be >= -9007199254740991" };
                if (vErrors === null) {
                  vErrors = [err6];
                } else {
                  vErrors.push(err6);
                }
                errors++;
              }
            }
          }
        }
        var _valid0 = _errs7 === errors;
        valid0 = valid0 || _valid0;
        if (!valid0) {
          const _errs9 = errors;
          if (errors === _errs9) {
            if (Array.isArray(data)) {
              if (data.length > 1e4) {
                const err7 = { instancePath, schemaPath: "#/anyOf/4/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" };
                if (vErrors === null) {
                  vErrors = [err7];
                } else {
                  vErrors.push(err7);
                }
                errors++;
              } else {
                var valid1 = true;
                const len0 = data.length;
                for (let i0 = 0;i0 < len0; i0++) {
                  const _errs11 = errors;
                  if (!wrapper0.validate(data[i0], { instancePath: instancePath + "/" + i0, parentData: data, parentDataProperty: i0, rootData })) {
                    vErrors = vErrors === null ? wrapper0.validate.errors : vErrors.concat(wrapper0.validate.errors);
                    errors = vErrors.length;
                  }
                  var valid1 = _errs11 === errors;
                  if (!valid1) {
                    break;
                  }
                }
              }
            } else {
              const err8 = { instancePath, schemaPath: "#/anyOf/4/type", keyword: "type", params: { type: "array" }, message: "must be array" };
              if (vErrors === null) {
                vErrors = [err8];
              } else {
                vErrors.push(err8);
              }
              errors++;
            }
          }
          var _valid0 = _errs9 === errors;
          valid0 = valid0 || _valid0;
          if (!valid0) {
            const _errs12 = errors;
            if (errors === _errs12) {
              if (data && typeof data == "object" && !Array.isArray(data)) {
                if (Object.keys(data).length > 1e4) {
                  const err9 = { instancePath, schemaPath: "#/anyOf/5/maxProperties", keyword: "maxProperties", params: { limit: 1e4 }, message: "must NOT have more than 10000 properties" };
                  if (vErrors === null) {
                    vErrors = [err9];
                  } else {
                    vErrors.push(err9);
                  }
                  errors++;
                } else {
                  for (const key0 in data) {
                    const _errs15 = errors;
                    if (!wrapper0.validate(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
                      vErrors = vErrors === null ? wrapper0.validate.errors : vErrors.concat(wrapper0.validate.errors);
                      errors = vErrors.length;
                    }
                    var valid2 = _errs15 === errors;
                    if (!valid2) {
                      break;
                    }
                  }
                }
              } else {
                const err10 = { instancePath, schemaPath: "#/anyOf/5/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                if (vErrors === null) {
                  vErrors = [err10];
                } else {
                  vErrors.push(err10);
                }
                errors++;
              }
            }
            var _valid0 = _errs12 === errors;
            valid0 = valid0 || _valid0;
          }
        }
      }
    }
  }
  if (!valid0) {
    const err11 = { instancePath, schemaPath: "#/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
    if (vErrors === null) {
      vErrors = [err11];
    } else {
      vErrors.push(err11);
    }
    errors++;
    validate97.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate97.errors = vErrors;
  return errors === 0;
}
function validate96(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      const _errs1 = errors;
      for (const key0 in data) {
        if (!(key0 === "parentIssue")) {
          const _errs2 = errors;
          if (!validate97(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
            vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
            errors = vErrors.length;
          }
          var valid0 = _errs2 === errors;
          if (!valid0) {
            break;
          }
        }
      }
      if (_errs1 === errors) {
        if (data.parentIssue !== undefined) {
          if (!validate42(data.parentIssue, { instancePath: instancePath + "/parentIssue", parentData: data, parentDataProperty: "parentIssue", rootData })) {
            vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
            errors = vErrors.length;
          }
        }
      }
    } else {
      validate96.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate96.errors = vErrors;
  return errors === 0;
}
function validate108(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id")) {
        validate108.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "covers")) {
            const _errs2 = errors;
            if (!validate97(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
              vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
            if (!valid0) {
              break;
            }
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            const _errs3 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid1 = _errs3 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.covers !== undefined) {
              let data2 = data.covers;
              const _errs4 = errors;
              if (errors === _errs4) {
                if (Array.isArray(data2)) {
                  if (data2.length > 2000) {
                    validate108.errors = [{ instancePath: instancePath + "/covers", schemaPath: "#/properties/covers/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                    return false;
                  } else {
                    var valid2 = true;
                    const len0 = data2.length;
                    for (let i0 = 0;i0 < len0; i0++) {
                      const _errs6 = errors;
                      if (!validate24(data2[i0], { instancePath: instancePath + "/covers/" + i0, parentData: data2, parentDataProperty: i0, rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var valid2 = _errs6 === errors;
                      if (!valid2) {
                        break;
                      }
                    }
                  }
                } else {
                  validate108.errors = [{ instancePath: instancePath + "/covers", schemaPath: "#/properties/covers/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid1 = _errs4 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      }
    } else {
      validate108.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate108.errors = vErrors;
  return errors === 0;
}
function validate106(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria")) {
        validate106.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "acceptanceCriteria" || key0 === "refinesSpec")) {
            const _errs2 = errors;
            if (!validate97(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
              vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
            if (!valid0) {
              break;
            }
          }
        }
        if (_errs1 === errors) {
          if (data.acceptanceCriteria !== undefined) {
            let data1 = data.acceptanceCriteria;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (Array.isArray(data1)) {
                if (data1.length > 2000) {
                  validate106.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                  return false;
                } else {
                  var valid2 = true;
                  const len0 = data1.length;
                  for (let i0 = 0;i0 < len0; i0++) {
                    const _errs5 = errors;
                    if (!validate108(data1[i0], { instancePath: instancePath + "/acceptanceCriteria/" + i0, parentData: data1, parentDataProperty: i0, rootData })) {
                      vErrors = vErrors === null ? validate108.errors : vErrors.concat(validate108.errors);
                      errors = vErrors.length;
                    }
                    var valid2 = _errs5 === errors;
                    if (!valid2) {
                      break;
                    }
                  }
                }
              } else {
                validate106.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                return false;
              }
            }
            var valid1 = _errs3 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.refinesSpec !== undefined) {
              const _errs6 = errors;
              if (!validate42(data.refinesSpec, { instancePath: instancePath + "/refinesSpec", parentData: data, parentDataProperty: "refinesSpec", rootData })) {
                vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs6 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      }
    } else {
      validate106.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate106.errors = vErrors;
  return errors === 0;
}
function validate120(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.appliesToSpec === undefined && (missing0 = "appliesToSpec")) {
        validate120.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "appliesToSpec" || key0 === "methodRef")) {
            const _errs2 = errors;
            if (!validate97(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
              vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
            if (!valid0) {
              break;
            }
          }
        }
        if (_errs1 === errors) {
          if (data.appliesToSpec !== undefined) {
            const _errs3 = errors;
            if (!validate42(data.appliesToSpec, { instancePath: instancePath + "/appliesToSpec", parentData: data, parentDataProperty: "appliesToSpec", rootData })) {
              vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
              errors = vErrors.length;
            }
            var valid1 = _errs3 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.methodRef !== undefined) {
              const _errs4 = errors;
              if (!validate42(data.methodRef, { instancePath: instancePath + "/methodRef", parentData: data, parentDataProperty: "methodRef", rootData })) {
                vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs4 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      }
    } else {
      validate120.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate120.errors = vErrors;
  return errors === 0;
}
function validate86(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (errors === _errs1) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.owner === undefined && (missing0 = "owner") || data.revision === undefined && (missing0 = "revision") || data.kind === undefined && (missing0 = "kind") || data.digest === undefined && (missing0 = "digest") || data.sourceRefs === undefined && (missing0 = "sourceRefs") || data.body === undefined && (missing0 = "body")) {
        const err0 = { instancePath, schemaPath: "#/oneOf/0/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        const _errs3 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "owner" || key0 === "revision" || key0 === "kind" || key0 === "digest" || key0 === "sourceRefs" || key0 === "body")) {
            const err1 = { instancePath, schemaPath: "#/oneOf/0/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
            break;
          }
        }
        if (_errs3 === errors) {
          if (data.id !== undefined) {
            const _errs4 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.owner !== undefined) {
              const _errs5 = errors;
              if (!validate44(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.revision !== undefined) {
                const _errs6 = errors;
                if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid1 = _errs6 === errors;
              } else {
                var valid1 = true;
              }
              if (valid1) {
                if (data.kind !== undefined) {
                  const _errs7 = errors;
                  if (data.kind !== "issue") {
                    const err2 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/0/properties/kind/const", keyword: "const", params: { allowedValue: "issue" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err2];
                    } else {
                      vErrors.push(err2);
                    }
                    errors++;
                  }
                  var valid1 = _errs7 === errors;
                } else {
                  var valid1 = true;
                }
                if (valid1) {
                  if (data.digest !== undefined) {
                    const _errs8 = errors;
                    if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid1 = _errs8 === errors;
                  } else {
                    var valid1 = true;
                  }
                  if (valid1) {
                    if (data.sourceRefs !== undefined) {
                      let data5 = data.sourceRefs;
                      const _errs9 = errors;
                      if (errors === _errs9) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2000) {
                            const err3 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err3];
                            } else {
                              vErrors.push(err3);
                            }
                            errors++;
                          } else {
                            var valid2 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs11 = errors;
                              if (!validate91(data5[i0], { instancePath: instancePath + "/sourceRefs/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                                errors = vErrors.length;
                              }
                              var valid2 = _errs11 === errors;
                              if (!valid2) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err4 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err4];
                          } else {
                            vErrors.push(err4);
                          }
                          errors++;
                        }
                      }
                      var valid1 = _errs9 === errors;
                    } else {
                      var valid1 = true;
                    }
                    if (valid1) {
                      if (data.body !== undefined) {
                        const _errs12 = errors;
                        if (!validate96(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate96.errors : vErrors.concat(validate96.errors);
                          errors = vErrors.length;
                        }
                        var valid1 = _errs12 === errors;
                      } else {
                        var valid1 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      const err5 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err5];
      } else {
        vErrors.push(err5);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs13 = errors;
  if (errors === _errs13) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.id === undefined && (missing1 = "id") || data.owner === undefined && (missing1 = "owner") || data.revision === undefined && (missing1 = "revision") || data.kind === undefined && (missing1 = "kind") || data.digest === undefined && (missing1 = "digest") || data.sourceRefs === undefined && (missing1 = "sourceRefs") || data.body === undefined && (missing1 = "body")) {
        const err6 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err6];
        } else {
          vErrors.push(err6);
        }
        errors++;
      } else {
        const _errs15 = errors;
        for (const key1 in data) {
          if (!(key1 === "id" || key1 === "owner" || key1 === "revision" || key1 === "kind" || key1 === "digest" || key1 === "sourceRefs" || key1 === "body")) {
            const err7 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err7];
            } else {
              vErrors.push(err7);
            }
            errors++;
            break;
          }
        }
        if (_errs15 === errors) {
          if (data.id !== undefined) {
            const _errs16 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid3 = _errs16 === errors;
          } else {
            var valid3 = true;
          }
          if (valid3) {
            if (data.owner !== undefined) {
              const _errs17 = errors;
              if (!validate44(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
                errors = vErrors.length;
              }
              var valid3 = _errs17 === errors;
            } else {
              var valid3 = true;
            }
            if (valid3) {
              if (data.revision !== undefined) {
                const _errs18 = errors;
                if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid3 = _errs18 === errors;
              } else {
                var valid3 = true;
              }
              if (valid3) {
                if (data.kind !== undefined) {
                  const _errs19 = errors;
                  if (data.kind !== "spec") {
                    const err8 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "spec" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err8];
                    } else {
                      vErrors.push(err8);
                    }
                    errors++;
                  }
                  var valid3 = _errs19 === errors;
                } else {
                  var valid3 = true;
                }
                if (valid3) {
                  if (data.digest !== undefined) {
                    const _errs20 = errors;
                    if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid3 = _errs20 === errors;
                  } else {
                    var valid3 = true;
                  }
                  if (valid3) {
                    if (data.sourceRefs !== undefined) {
                      let data13 = data.sourceRefs;
                      const _errs21 = errors;
                      if (errors === _errs21) {
                        if (Array.isArray(data13)) {
                          if (data13.length > 2000) {
                            const err9 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err9];
                            } else {
                              vErrors.push(err9);
                            }
                            errors++;
                          } else {
                            var valid4 = true;
                            const len1 = data13.length;
                            for (let i1 = 0;i1 < len1; i1++) {
                              const _errs23 = errors;
                              if (!validate91(data13[i1], { instancePath: instancePath + "/sourceRefs/" + i1, parentData: data13, parentDataProperty: i1, rootData })) {
                                vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                                errors = vErrors.length;
                              }
                              var valid4 = _errs23 === errors;
                              if (!valid4) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err10 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err10];
                          } else {
                            vErrors.push(err10);
                          }
                          errors++;
                        }
                      }
                      var valid3 = _errs21 === errors;
                    } else {
                      var valid3 = true;
                    }
                    if (valid3) {
                      if (data.body !== undefined) {
                        const _errs24 = errors;
                        if (!validate106(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate106.errors : vErrors.concat(validate106.errors);
                          errors = vErrors.length;
                        }
                        var valid3 = _errs24 === errors;
                      } else {
                        var valid3 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      const err11 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err11];
      } else {
        vErrors.push(err11);
      }
      errors++;
    }
  }
  var _valid0 = _errs13 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
    const _errs25 = errors;
    if (errors === _errs25) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.id === undefined && (missing2 = "id") || data.owner === undefined && (missing2 = "owner") || data.revision === undefined && (missing2 = "revision") || data.kind === undefined && (missing2 = "kind") || data.digest === undefined && (missing2 = "digest") || data.sourceRefs === undefined && (missing2 = "sourceRefs") || data.body === undefined && (missing2 = "body")) {
          const err12 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err12];
          } else {
            vErrors.push(err12);
          }
          errors++;
        } else {
          const _errs27 = errors;
          for (const key2 in data) {
            if (!(key2 === "id" || key2 === "owner" || key2 === "revision" || key2 === "kind" || key2 === "digest" || key2 === "sourceRefs" || key2 === "body")) {
              const err13 = { instancePath, schemaPath: "#/oneOf/2/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" };
              if (vErrors === null) {
                vErrors = [err13];
              } else {
                vErrors.push(err13);
              }
              errors++;
              break;
            }
          }
          if (_errs27 === errors) {
            if (data.id !== undefined) {
              const _errs28 = errors;
              if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid5 = _errs28 === errors;
            } else {
              var valid5 = true;
            }
            if (valid5) {
              if (data.owner !== undefined) {
                const _errs29 = errors;
                if (!validate44(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                  vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
                  errors = vErrors.length;
                }
                var valid5 = _errs29 === errors;
              } else {
                var valid5 = true;
              }
              if (valid5) {
                if (data.revision !== undefined) {
                  const _errs30 = errors;
                  if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                    vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                    errors = vErrors.length;
                  }
                  var valid5 = _errs30 === errors;
                } else {
                  var valid5 = true;
                }
                if (valid5) {
                  if (data.kind !== undefined) {
                    const _errs31 = errors;
                    if (data.kind !== "plan") {
                      const err14 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/2/properties/kind/const", keyword: "const", params: { allowedValue: "plan" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err14];
                      } else {
                        vErrors.push(err14);
                      }
                      errors++;
                    }
                    var valid5 = _errs31 === errors;
                  } else {
                    var valid5 = true;
                  }
                  if (valid5) {
                    if (data.digest !== undefined) {
                      const _errs32 = errors;
                      if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                        vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                        errors = vErrors.length;
                      }
                      var valid5 = _errs32 === errors;
                    } else {
                      var valid5 = true;
                    }
                    if (valid5) {
                      if (data.sourceRefs !== undefined) {
                        let data21 = data.sourceRefs;
                        const _errs33 = errors;
                        if (errors === _errs33) {
                          if (Array.isArray(data21)) {
                            if (data21.length > 2000) {
                              const err15 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                              if (vErrors === null) {
                                vErrors = [err15];
                              } else {
                                vErrors.push(err15);
                              }
                              errors++;
                            } else {
                              var valid6 = true;
                              const len2 = data21.length;
                              for (let i2 = 0;i2 < len2; i2++) {
                                const _errs35 = errors;
                                if (!validate91(data21[i2], { instancePath: instancePath + "/sourceRefs/" + i2, parentData: data21, parentDataProperty: i2, rootData })) {
                                  vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                                  errors = vErrors.length;
                                }
                                var valid6 = _errs35 === errors;
                                if (!valid6) {
                                  break;
                                }
                              }
                            }
                          } else {
                            const err16 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                            if (vErrors === null) {
                              vErrors = [err16];
                            } else {
                              vErrors.push(err16);
                            }
                            errors++;
                          }
                        }
                        var valid5 = _errs33 === errors;
                      } else {
                        var valid5 = true;
                      }
                      if (valid5) {
                        if (data.body !== undefined) {
                          const _errs36 = errors;
                          if (!validate120(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                            vErrors = vErrors === null ? validate120.errors : vErrors.concat(validate120.errors);
                            errors = vErrors.length;
                          }
                          var valid5 = _errs36 === errors;
                        } else {
                          var valid5 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      } else {
        const err17 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err17];
        } else {
          vErrors.push(err17);
        }
        errors++;
      }
    }
    var _valid0 = _errs25 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
      }
    }
  }
  if (!valid0) {
    const err18 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err18];
    } else {
      vErrors.push(err18);
    }
    errors++;
    validate86.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate86.errors = vErrors;
  return errors === 0;
}
var schema29 = { type: "object", properties: { producer: { $ref: "#/$defs/TaskRef" }, consumer: { $ref: "#/$defs/TaskRef" }, required: { enum: ["verified", "accepted"] }, artifact: { $ref: "#/$defs/ArtifactRef" } }, required: ["producer", "consumer", "required"], additionalProperties: false };
function validate127(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.taskId === undefined && (missing0 = "taskId")) {
        validate127.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId" || key0 === "taskId")) {
            validate127.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.taskId, { instancePath: instancePath + "/taskId", parentData: data, parentDataProperty: "taskId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate127.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate127.errors = vErrors;
  return errors === 0;
}
function validate133(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.revision === undefined && (missing0 = "revision") || data.digest === undefined && (missing0 = "digest")) {
        validate133.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "revision" || key0 === "digest")) {
            validate133.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.revision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.digest !== undefined) {
                const _errs4 = errors;
                if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                  vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate133.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate133.errors = vErrors;
  return errors === 0;
}
function validate126(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.producer === undefined && (missing0 = "producer") || data.consumer === undefined && (missing0 = "consumer") || data.required === undefined && (missing0 = "required")) {
        validate126.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "producer" || key0 === "consumer" || key0 === "required" || key0 === "artifact")) {
            validate126.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.producer !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.consumer !== undefined) {
              const _errs3 = errors;
              if (!validate127(data.consumer, { instancePath: instancePath + "/consumer", parentData: data, parentDataProperty: "consumer", rootData })) {
                vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.required !== undefined) {
                let data2 = data.required;
                const _errs4 = errors;
                if (!(data2 === "verified" || data2 === "accepted")) {
                  validate126.errors = [{ instancePath: instancePath + "/required", schemaPath: "#/properties/required/enum", keyword: "enum", params: { allowedValues: schema29.properties.required.enum }, message: "must be equal to one of the allowed values" }];
                  return false;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.artifact !== undefined) {
                  const _errs5 = errors;
                  if (!validate133(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                    vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate126.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate126.errors = vErrors;
  return errors === 0;
}
function validate140(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (errors === _errs1) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.kind === undefined && (missing0 = "kind") || data.userId === undefined && (missing0 = "userId")) {
        const err0 = { instancePath, schemaPath: "#/oneOf/0/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        const _errs3 = errors;
        for (const key0 in data) {
          if (!(key0 === "kind" || key0 === "userId")) {
            const err1 = { instancePath, schemaPath: "#/oneOf/0/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
            break;
          }
        }
        if (_errs3 === errors) {
          if (data.kind !== undefined) {
            const _errs4 = errors;
            if (data.kind !== "user") {
              const err2 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/0/properties/kind/const", keyword: "const", params: { allowedValue: "user" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err2];
              } else {
                vErrors.push(err2);
              }
              errors++;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.userId !== undefined) {
              const _errs5 = errors;
              if (!validate24(data.userId, { instancePath: instancePath + "/userId", parentData: data, parentDataProperty: "userId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      }
    } else {
      const err3 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err3];
      } else {
        vErrors.push(err3);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs6 = errors;
  if (errors === _errs6) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.kind === undefined && (missing1 = "kind") || data.domainId === undefined && (missing1 = "domainId")) {
        const err4 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err4];
        } else {
          vErrors.push(err4);
        }
        errors++;
      } else {
        const _errs8 = errors;
        for (const key1 in data) {
          if (!(key1 === "kind" || key1 === "domainId")) {
            const err5 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err5];
            } else {
              vErrors.push(err5);
            }
            errors++;
            break;
          }
        }
        if (_errs8 === errors) {
          if (data.kind !== undefined) {
            const _errs9 = errors;
            if (data.kind !== "domain") {
              const err6 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "domain" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err6];
              } else {
                vErrors.push(err6);
              }
              errors++;
            }
            var valid2 = _errs9 === errors;
          } else {
            var valid2 = true;
          }
          if (valid2) {
            if (data.domainId !== undefined) {
              const _errs10 = errors;
              if (!validate24(data.domainId, { instancePath: instancePath + "/domainId", parentData: data, parentDataProperty: "domainId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid2 = _errs10 === errors;
            } else {
              var valid2 = true;
            }
          }
        }
      }
    } else {
      const err7 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err7];
      } else {
        vErrors.push(err7);
      }
      errors++;
    }
  }
  var _valid0 = _errs6 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
    const _errs11 = errors;
    if (errors === _errs11) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.kind === undefined && (missing2 = "kind") || data.projectId === undefined && (missing2 = "projectId")) {
          const err8 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err8];
          } else {
            vErrors.push(err8);
          }
          errors++;
        } else {
          const _errs13 = errors;
          for (const key2 in data) {
            if (!(key2 === "kind" || key2 === "projectId")) {
              const err9 = { instancePath, schemaPath: "#/oneOf/2/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" };
              if (vErrors === null) {
                vErrors = [err9];
              } else {
                vErrors.push(err9);
              }
              errors++;
              break;
            }
          }
          if (_errs13 === errors) {
            if (data.kind !== undefined) {
              const _errs14 = errors;
              if (data.kind !== "project") {
                const err10 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/2/properties/kind/const", keyword: "const", params: { allowedValue: "project" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err10];
                } else {
                  vErrors.push(err10);
                }
                errors++;
              }
              var valid3 = _errs14 === errors;
            } else {
              var valid3 = true;
            }
            if (valid3) {
              if (data.projectId !== undefined) {
                const _errs15 = errors;
                if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid3 = _errs15 === errors;
              } else {
                var valid3 = true;
              }
            }
          }
        }
      } else {
        const err11 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err11];
        } else {
          vErrors.push(err11);
        }
        errors++;
      }
    }
    var _valid0 = _errs11 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
      }
      const _errs16 = errors;
      if (errors === _errs16) {
        if (data && typeof data == "object" && !Array.isArray(data)) {
          let missing3;
          if (data.kind === undefined && (missing3 = "kind") || data.projectId === undefined && (missing3 = "projectId") || data.workId === undefined && (missing3 = "workId")) {
            const err12 = { instancePath, schemaPath: "#/oneOf/3/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" };
            if (vErrors === null) {
              vErrors = [err12];
            } else {
              vErrors.push(err12);
            }
            errors++;
          } else {
            const _errs18 = errors;
            for (const key3 in data) {
              if (!(key3 === "kind" || key3 === "projectId" || key3 === "workId")) {
                const err13 = { instancePath, schemaPath: "#/oneOf/3/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key3 }, message: "must NOT have additional properties" };
                if (vErrors === null) {
                  vErrors = [err13];
                } else {
                  vErrors.push(err13);
                }
                errors++;
                break;
              }
            }
            if (_errs18 === errors) {
              if (data.kind !== undefined) {
                const _errs19 = errors;
                if (data.kind !== "work") {
                  const err14 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/3/properties/kind/const", keyword: "const", params: { allowedValue: "work" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err14];
                  } else {
                    vErrors.push(err14);
                  }
                  errors++;
                }
                var valid4 = _errs19 === errors;
              } else {
                var valid4 = true;
              }
              if (valid4) {
                if (data.projectId !== undefined) {
                  const _errs20 = errors;
                  if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid4 = _errs20 === errors;
                } else {
                  var valid4 = true;
                }
                if (valid4) {
                  if (data.workId !== undefined) {
                    const _errs21 = errors;
                    if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var valid4 = _errs21 === errors;
                  } else {
                    var valid4 = true;
                  }
                }
              }
            }
          }
        } else {
          const err15 = { instancePath, schemaPath: "#/oneOf/3/type", keyword: "type", params: { type: "object" }, message: "must be object" };
          if (vErrors === null) {
            vErrors = [err15];
          } else {
            vErrors.push(err15);
          }
          errors++;
        }
      }
      var _valid0 = _errs16 === errors;
      if (_valid0 && valid0) {
        valid0 = false;
        passing0 = [passing0, 3];
      } else {
        if (_valid0) {
          valid0 = true;
          passing0 = 3;
        }
        const _errs22 = errors;
        if (errors === _errs22) {
          if (data && typeof data == "object" && !Array.isArray(data)) {
            let missing4;
            if (data.kind === undefined && (missing4 = "kind") || data.projectId === undefined && (missing4 = "projectId") || data.workId === undefined && (missing4 = "workId") || data.taskId === undefined && (missing4 = "taskId")) {
              const err16 = { instancePath, schemaPath: "#/oneOf/4/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" };
              if (vErrors === null) {
                vErrors = [err16];
              } else {
                vErrors.push(err16);
              }
              errors++;
            } else {
              const _errs24 = errors;
              for (const key4 in data) {
                if (!(key4 === "kind" || key4 === "projectId" || key4 === "workId" || key4 === "taskId")) {
                  const err17 = { instancePath, schemaPath: "#/oneOf/4/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key4 }, message: "must NOT have additional properties" };
                  if (vErrors === null) {
                    vErrors = [err17];
                  } else {
                    vErrors.push(err17);
                  }
                  errors++;
                  break;
                }
              }
              if (_errs24 === errors) {
                if (data.kind !== undefined) {
                  const _errs25 = errors;
                  if (data.kind !== "task") {
                    const err18 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/4/properties/kind/const", keyword: "const", params: { allowedValue: "task" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err18];
                    } else {
                      vErrors.push(err18);
                    }
                    errors++;
                  }
                  var valid5 = _errs25 === errors;
                } else {
                  var valid5 = true;
                }
                if (valid5) {
                  if (data.projectId !== undefined) {
                    const _errs26 = errors;
                    if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var valid5 = _errs26 === errors;
                  } else {
                    var valid5 = true;
                  }
                  if (valid5) {
                    if (data.workId !== undefined) {
                      const _errs27 = errors;
                      if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var valid5 = _errs27 === errors;
                    } else {
                      var valid5 = true;
                    }
                    if (valid5) {
                      if (data.taskId !== undefined) {
                        const _errs28 = errors;
                        if (!validate24(data.taskId, { instancePath: instancePath + "/taskId", parentData: data, parentDataProperty: "taskId", rootData })) {
                          vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                          errors = vErrors.length;
                        }
                        var valid5 = _errs28 === errors;
                      } else {
                        var valid5 = true;
                      }
                    }
                  }
                }
              }
            }
          } else {
            const err19 = { instancePath, schemaPath: "#/oneOf/4/type", keyword: "type", params: { type: "object" }, message: "must be object" };
            if (vErrors === null) {
              vErrors = [err19];
            } else {
              vErrors.push(err19);
            }
            errors++;
          }
        }
        var _valid0 = _errs22 === errors;
        if (_valid0 && valid0) {
          valid0 = false;
          passing0 = [passing0, 4];
        } else {
          if (_valid0) {
            valid0 = true;
            passing0 = 4;
          }
        }
      }
    }
  }
  if (!valid0) {
    const err20 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err20];
    } else {
      vErrors.push(err20);
    }
    errors++;
    validate140.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate140.errors = vErrors;
  return errors === 0;
}
function validate139(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.kind === undefined && (missing0 = "kind") || data.from === undefined && (missing0 = "from") || data.to === undefined && (missing0 = "to")) {
        validate139.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "kind" || key0 === "from" || key0 === "to")) {
            validate139.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.kind !== undefined) {
            const _errs2 = errors;
            if (data.kind !== "relatedTo") {
              validate139.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/const", keyword: "const", params: { allowedValue: "relatedTo" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.from !== undefined) {
              const _errs3 = errors;
              if (!validate140(data.from, { instancePath: instancePath + "/from", parentData: data, parentDataProperty: "from", rootData })) {
                vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.to !== undefined) {
                const _errs4 = errors;
                if (!validate140(data.to, { instancePath: instancePath + "/to", parentData: data, parentDataProperty: "to", rootData })) {
                  vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate139.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate139.errors = vErrors;
  return errors === 0;
}
function validate152(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.from === undefined && (missing0 = "from") || data.to === undefined && (missing0 = "to") || data.authorityRef === undefined && (missing0 = "authorityRef")) {
        validate152.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "from" || key0 === "to" || key0 === "authorityRef")) {
            validate152.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.from !== undefined) {
            const _errs2 = errors;
            if (!validate140(data.from, { instancePath: instancePath + "/from", parentData: data, parentDataProperty: "from", rootData })) {
              vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.to !== undefined) {
              const _errs3 = errors;
              if (!validate140(data.to, { instancePath: instancePath + "/to", parentData: data, parentDataProperty: "to", rootData })) {
                vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.authorityRef !== undefined) {
                const _errs4 = errors;
                if (!validate91(data.authorityRef, { instancePath: instancePath + "/authorityRef", parentData: data, parentDataProperty: "authorityRef", rootData })) {
                  vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate152.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate152.errors = vErrors;
  return errors === 0;
}
function validate157(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.subject === undefined && (missing0 = "subject") || data.source === undefined && (missing0 = "source")) {
        validate157.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "subject" || key0 === "source")) {
            validate157.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.subject !== undefined) {
            const _errs2 = errors;
            if (!validate140(data.subject, { instancePath: instancePath + "/subject", parentData: data, parentDataProperty: "subject", rootData })) {
              vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.source !== undefined) {
              const _errs3 = errors;
              if (!validate91(data.source, { instancePath: instancePath + "/source", parentData: data, parentDataProperty: "source", rootData })) {
                vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate157.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate157.errors = vErrors;
  return errors === 0;
}
function validate163(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.runtimeId === undefined && (missing0 = "runtimeId") || data.nativeSessionId === undefined && (missing0 = "nativeSessionId") || data.nativeTaskId === undefined && (missing0 = "nativeTaskId") || data.nativeRunId === undefined && (missing0 = "nativeRunId") || data.paneId === undefined && (missing0 = "paneId")) {
        validate163.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "runtimeId" || key0 === "nativeSessionId" || key0 === "nativeTaskId" || key0 === "nativeRunId" || key0 === "paneId")) {
            validate163.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.runtimeId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.runtimeId, { instancePath: instancePath + "/runtimeId", parentData: data, parentDataProperty: "runtimeId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.nativeSessionId !== undefined) {
              let data1 = data.nativeSessionId;
              const _errs3 = errors;
              const _errs4 = errors;
              let valid1 = false;
              const _errs5 = errors;
              if (!validate24(data1, { instancePath: instancePath + "/nativeSessionId", parentData: data, parentDataProperty: "nativeSessionId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var _valid0 = _errs5 === errors;
              valid1 = valid1 || _valid0;
              if (!valid1) {
                const _errs6 = errors;
                if (data1 !== null) {
                  const err0 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/properties/nativeSessionId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                  if (vErrors === null) {
                    vErrors = [err0];
                  } else {
                    vErrors.push(err0);
                  }
                  errors++;
                }
                var _valid0 = _errs6 === errors;
                valid1 = valid1 || _valid0;
              }
              if (!valid1) {
                const err1 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/properties/nativeSessionId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                if (vErrors === null) {
                  vErrors = [err1];
                } else {
                  vErrors.push(err1);
                }
                errors++;
                validate163.errors = vErrors;
                return false;
              } else {
                errors = _errs4;
                if (vErrors !== null) {
                  if (_errs4) {
                    vErrors.length = _errs4;
                  } else {
                    vErrors = null;
                  }
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.nativeTaskId !== undefined) {
                let data2 = data.nativeTaskId;
                const _errs8 = errors;
                const _errs9 = errors;
                let valid2 = false;
                const _errs10 = errors;
                if (!validate24(data2, { instancePath: instancePath + "/nativeTaskId", parentData: data, parentDataProperty: "nativeTaskId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var _valid1 = _errs10 === errors;
                valid2 = valid2 || _valid1;
                if (!valid2) {
                  const _errs11 = errors;
                  if (data2 !== null) {
                    const err2 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/properties/nativeTaskId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                    if (vErrors === null) {
                      vErrors = [err2];
                    } else {
                      vErrors.push(err2);
                    }
                    errors++;
                  }
                  var _valid1 = _errs11 === errors;
                  valid2 = valid2 || _valid1;
                }
                if (!valid2) {
                  const err3 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/properties/nativeTaskId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                  if (vErrors === null) {
                    vErrors = [err3];
                  } else {
                    vErrors.push(err3);
                  }
                  errors++;
                  validate163.errors = vErrors;
                  return false;
                } else {
                  errors = _errs9;
                  if (vErrors !== null) {
                    if (_errs9) {
                      vErrors.length = _errs9;
                    } else {
                      vErrors = null;
                    }
                  }
                }
                var valid0 = _errs8 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.nativeRunId !== undefined) {
                  let data3 = data.nativeRunId;
                  const _errs13 = errors;
                  const _errs14 = errors;
                  let valid3 = false;
                  const _errs15 = errors;
                  if (!validate24(data3, { instancePath: instancePath + "/nativeRunId", parentData: data, parentDataProperty: "nativeRunId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var _valid2 = _errs15 === errors;
                  valid3 = valid3 || _valid2;
                  if (!valid3) {
                    const _errs16 = errors;
                    if (data3 !== null) {
                      const err4 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/properties/nativeRunId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                      if (vErrors === null) {
                        vErrors = [err4];
                      } else {
                        vErrors.push(err4);
                      }
                      errors++;
                    }
                    var _valid2 = _errs16 === errors;
                    valid3 = valid3 || _valid2;
                  }
                  if (!valid3) {
                    const err5 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/properties/nativeRunId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                    if (vErrors === null) {
                      vErrors = [err5];
                    } else {
                      vErrors.push(err5);
                    }
                    errors++;
                    validate163.errors = vErrors;
                    return false;
                  } else {
                    errors = _errs14;
                    if (vErrors !== null) {
                      if (_errs14) {
                        vErrors.length = _errs14;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                  var valid0 = _errs13 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.paneId !== undefined) {
                    let data4 = data.paneId;
                    const _errs18 = errors;
                    const _errs19 = errors;
                    let valid4 = false;
                    const _errs20 = errors;
                    if (!validate24(data4, { instancePath: instancePath + "/paneId", parentData: data, parentDataProperty: "paneId", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var _valid3 = _errs20 === errors;
                    valid4 = valid4 || _valid3;
                    if (!valid4) {
                      const _errs21 = errors;
                      if (data4 !== null) {
                        const err6 = { instancePath: instancePath + "/paneId", schemaPath: "#/properties/paneId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                        if (vErrors === null) {
                          vErrors = [err6];
                        } else {
                          vErrors.push(err6);
                        }
                        errors++;
                      }
                      var _valid3 = _errs21 === errors;
                      valid4 = valid4 || _valid3;
                    }
                    if (!valid4) {
                      const err7 = { instancePath: instancePath + "/paneId", schemaPath: "#/properties/paneId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                      if (vErrors === null) {
                        vErrors = [err7];
                      } else {
                        vErrors.push(err7);
                      }
                      errors++;
                      validate163.errors = vErrors;
                      return false;
                    } else {
                      errors = _errs19;
                      if (vErrors !== null) {
                        if (_errs19) {
                          vErrors.length = _errs19;
                        } else {
                          vErrors = null;
                        }
                      }
                    }
                    var valid0 = _errs18 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate163.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate163.errors = vErrors;
  return errors === 0;
}
function validate161(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.binding === undefined && (missing0 = "binding")) {
        validate161.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "binding")) {
            validate161.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.binding !== undefined) {
              const _errs3 = errors;
              if (!validate163(data.binding, { instancePath: instancePath + "/binding", parentData: data, parentDataProperty: "binding", rootData })) {
                vErrors = vErrors === null ? validate163.errors : vErrors.concat(validate163.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate161.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate161.errors = vErrors;
  return errors === 0;
}
var schema39 = { type: "object", properties: { kind: { enum: ["work", "task", "run", "issue", "spec", "plan"] }, scope: { $ref: "#/$defs/Scope" }, key: { type: "string", minLength: 1, maxLength: 4096 } }, required: ["kind", "scope", "key"], additionalProperties: false };
function validate173(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.kind === undefined && (missing0 = "kind") || data.scope === undefined && (missing0 = "scope") || data.key === undefined && (missing0 = "key")) {
        validate173.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "kind" || key0 === "scope" || key0 === "key")) {
            validate173.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.kind !== undefined) {
            let data0 = data.kind;
            const _errs2 = errors;
            if (!(data0 === "work" || data0 === "task" || data0 === "run" || data0 === "issue" || data0 === "spec" || data0 === "plan")) {
              validate173.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema39.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.scope !== undefined) {
              const _errs3 = errors;
              if (!validate140(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData })) {
                vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.key !== undefined) {
                let data2 = data.key;
                const _errs4 = errors;
                if (errors === _errs4) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 4096) {
                      validate173.errors = [{ instancePath: instancePath + "/key", schemaPath: "#/properties/key/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate173.errors = [{ instancePath: instancePath + "/key", schemaPath: "#/properties/key/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      }
                    }
                  } else {
                    validate173.errors = [{ instancePath: instancePath + "/key", schemaPath: "#/properties/key/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate173.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate173.errors = vErrors;
  return errors === 0;
}
function validate171(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.namespace === undefined && (missing0 = "namespace") || data.from === undefined && (missing0 = "from") || data.to === undefined && (missing0 = "to") || data.revision === undefined && (missing0 = "revision") || data.provenance === undefined && (missing0 = "provenance") || data.reason === undefined && (missing0 = "reason")) {
        validate171.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "namespace" || key0 === "from" || key0 === "to" || key0 === "revision" || key0 === "provenance" || key0 === "reason")) {
            validate171.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.namespace !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.namespace, { instancePath: instancePath + "/namespace", parentData: data, parentDataProperty: "namespace", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.from !== undefined) {
              const _errs3 = errors;
              if (!validate173(data.from, { instancePath: instancePath + "/from", parentData: data, parentDataProperty: "from", rootData })) {
                vErrors = vErrors === null ? validate173.errors : vErrors.concat(validate173.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.to !== undefined) {
                const _errs4 = errors;
                if (!validate173(data.to, { instancePath: instancePath + "/to", parentData: data, parentDataProperty: "to", rootData })) {
                  vErrors = vErrors === null ? validate173.errors : vErrors.concat(validate173.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.revision !== undefined) {
                  const _errs5 = errors;
                  if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                    vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.provenance !== undefined) {
                    const _errs6 = errors;
                    if (!validate58(data.provenance, { instancePath: instancePath + "/provenance", parentData: data, parentDataProperty: "provenance", rootData })) {
                      vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.reason !== undefined) {
                      let data5 = data.reason;
                      const _errs7 = errors;
                      if (errors === _errs7) {
                        if (typeof data5 === "string") {
                          if (func3(data5) > 4096) {
                            validate171.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                            return false;
                          } else {
                            if (func3(data5) < 1) {
                              validate171.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            }
                          }
                        } else {
                          validate171.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate171.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate171.errors = vErrors;
  return errors === 0;
}
var schema40 = { type: "object", properties: { schema: { const: "common.git-selected-external-artifact.v3" }, producer: { $ref: "#/$defs/ProducerContractBinding" }, producerContext: { $ref: "#/$defs/GitSourcePin" }, artifact: { $ref: "#/$defs/ArtifactRef" }, artifactSource: { $ref: "#/$defs/GitSourcePin" }, producerMilestone: { $ref: "#/$defs/GitSourcePin" }, selection: { $ref: "#/$defs/ArtifactGateSelection" }, selectionReceipt: { $ref: "#/$defs/GitSourcePin" }, criteria: { type: "array", items: { type: "object", properties: { criterionId: { $ref: "#/$defs/Id" }, receipt: { $ref: "#/$defs/GitSourcePin" } }, required: ["criterionId", "receipt"], additionalProperties: false }, maxItems: 2048 }, receiverOwnerId: { $ref: "#/$defs/Id" }, receiverUse: { $ref: "#/$defs/GitSourcePin" }, producerSources: { $ref: "#/$defs/TaskContractSourceClosure" }, consumerSources: { $ref: "#/$defs/TaskContractSourceClosure" } }, required: ["schema", "producer", "producerContext", "artifact", "artifactSource", "producerMilestone", "selection", "selectionReceipt", "criteria", "receiverOwnerId", "receiverUse", "producerSources", "consumerSources"], additionalProperties: false };
function validate181(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.contracts === undefined && (missing0 = "contracts") || data.effectiveDigest === undefined && (missing0 = "effectiveDigest")) {
        validate181.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "workRevision" || key0 === "taskRevision" || key0 === "workContracts" || key0 === "contracts" || key0 === "effectiveDigest")) {
            validate181.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workRevision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.workRevision, { instancePath: instancePath + "/workRevision", parentData: data, parentDataProperty: "workRevision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskRevision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.taskRevision, { instancePath: instancePath + "/taskRevision", parentData: data, parentDataProperty: "taskRevision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.workContracts !== undefined) {
                  const _errs5 = errors;
                  if (!validate41(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                    vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.contracts !== undefined) {
                    const _errs6 = errors;
                    if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                      vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.effectiveDigest !== undefined) {
                      const _errs7 = errors;
                      if (!validate52(data.effectiveDigest, { instancePath: instancePath + "/effectiveDigest", parentData: data, parentDataProperty: "effectiveDigest", rootData })) {
                        vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate181.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate181.errors = vErrors;
  return errors === 0;
}
function validate197(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.contracts === undefined && (missing0 = "contracts") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria")) {
        validate197.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "workRevision" || key0 === "taskRevision" || key0 === "workContracts" || key0 === "contracts" || key0 === "acceptanceCriteria")) {
            validate197.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workRevision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.workRevision, { instancePath: instancePath + "/workRevision", parentData: data, parentDataProperty: "workRevision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskRevision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.taskRevision, { instancePath: instancePath + "/taskRevision", parentData: data, parentDataProperty: "taskRevision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.workContracts !== undefined) {
                  const _errs5 = errors;
                  if (!validate41(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                    vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.contracts !== undefined) {
                    const _errs6 = errors;
                    if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                      vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.acceptanceCriteria !== undefined) {
                      let data5 = data.acceptanceCriteria;
                      const _errs7 = errors;
                      if (errors === _errs7) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2048) {
                            validate197.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                            return false;
                          } else {
                            var valid1 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs9 = errors;
                              if (!validate24(data5[i0], { instancePath: instancePath + "/acceptanceCriteria/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                errors = vErrors.length;
                              }
                              var valid1 = _errs9 === errors;
                              if (!valid1) {
                                break;
                              }
                            }
                          }
                        } else {
                          validate197.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                          return false;
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate197.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate197.errors = vErrors;
  return errors === 0;
}
function validate193(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.instanceId === undefined && (missing0 = "instanceId") || data.revision === undefined && (missing0 = "revision") || data.definition === undefined && (missing0 = "definition") || data.consumer === undefined && (missing0 = "consumer") || data.selectedAlternative === undefined && (missing0 = "selectedAlternative") || data.selectionOwnerId === undefined && (missing0 = "selectionOwnerId")) {
        validate193.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "instanceId" || key0 === "revision" || key0 === "definition" || key0 === "consumer" || key0 === "selectedAlternative" || key0 === "selectionOwnerId")) {
            validate193.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.instanceId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.instanceId, { instancePath: instancePath + "/instanceId", parentData: data, parentDataProperty: "instanceId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.revision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.definition !== undefined) {
                const _errs4 = errors;
                if (!validate58(data.definition, { instancePath: instancePath + "/definition", parentData: data, parentDataProperty: "definition", rootData })) {
                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.consumer !== undefined) {
                  const _errs5 = errors;
                  if (!validate197(data.consumer, { instancePath: instancePath + "/consumer", parentData: data, parentDataProperty: "consumer", rootData })) {
                    vErrors = vErrors === null ? validate197.errors : vErrors.concat(validate197.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.selectedAlternative !== undefined) {
                    const _errs6 = errors;
                    if (!validate24(data.selectedAlternative, { instancePath: instancePath + "/selectedAlternative", parentData: data, parentDataProperty: "selectedAlternative", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.selectionOwnerId !== undefined) {
                      const _errs7 = errors;
                      if (!validate24(data.selectionOwnerId, { instancePath: instancePath + "/selectionOwnerId", parentData: data, parentDataProperty: "selectionOwnerId", rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate193.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate193.errors = vErrors;
  return errors === 0;
}
function validate213(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.work === undefined && (missing0 = "work") || data.task === undefined && (missing0 = "task") || data.contracts === undefined && (missing0 = "contracts") || data.graph === undefined && (missing0 = "graph")) {
        validate213.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "work" || key0 === "task" || key0 === "contracts" || key0 === "graph")) {
            validate213.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.work !== undefined) {
            const _errs2 = errors;
            if (!validate58(data.work, { instancePath: instancePath + "/work", parentData: data, parentDataProperty: "work", rootData })) {
              vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs3 = errors;
              if (!validate58(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.contracts !== undefined) {
                let data2 = data.contracts;
                const _errs4 = errors;
                if (errors === _errs4) {
                  if (Array.isArray(data2)) {
                    if (data2.length > 2048) {
                      validate213.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                      return false;
                    } else {
                      var valid1 = true;
                      const len0 = data2.length;
                      for (let i0 = 0;i0 < len0; i0++) {
                        const _errs6 = errors;
                        if (!validate58(data2[i0], { instancePath: instancePath + "/contracts/" + i0, parentData: data2, parentDataProperty: i0, rootData })) {
                          vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                          errors = vErrors.length;
                        }
                        var valid1 = _errs6 === errors;
                        if (!valid1) {
                          break;
                        }
                      }
                    }
                  } else {
                    validate213.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                    return false;
                  }
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.graph !== undefined) {
                  const _errs7 = errors;
                  if (!validate58(data.graph, { instancePath: instancePath + "/graph", parentData: data, parentDataProperty: "graph", rootData })) {
                    vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate213.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate213.errors = vErrors;
  return errors === 0;
}
function validate180(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.producer === undefined && (missing0 = "producer") || data.producerContext === undefined && (missing0 = "producerContext") || data.artifact === undefined && (missing0 = "artifact") || data.artifactSource === undefined && (missing0 = "artifactSource") || data.producerMilestone === undefined && (missing0 = "producerMilestone") || data.selection === undefined && (missing0 = "selection") || data.selectionReceipt === undefined && (missing0 = "selectionReceipt") || data.criteria === undefined && (missing0 = "criteria") || data.receiverOwnerId === undefined && (missing0 = "receiverOwnerId") || data.receiverUse === undefined && (missing0 = "receiverUse") || data.producerSources === undefined && (missing0 = "producerSources") || data.consumerSources === undefined && (missing0 = "consumerSources")) {
        validate180.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema40.properties, key0)) {
            validate180.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-selected-external-artifact.v3") {
              validate180.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-selected-external-artifact.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.producer !== undefined) {
              const _errs3 = errors;
              if (!validate181(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
                vErrors = vErrors === null ? validate181.errors : vErrors.concat(validate181.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.producerContext !== undefined) {
                const _errs4 = errors;
                if (!validate58(data.producerContext, { instancePath: instancePath + "/producerContext", parentData: data, parentDataProperty: "producerContext", rootData })) {
                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.artifact !== undefined) {
                  const _errs5 = errors;
                  if (!validate133(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                    vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.artifactSource !== undefined) {
                    const _errs6 = errors;
                    if (!validate58(data.artifactSource, { instancePath: instancePath + "/artifactSource", parentData: data, parentDataProperty: "artifactSource", rootData })) {
                      vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.producerMilestone !== undefined) {
                      const _errs7 = errors;
                      if (!validate58(data.producerMilestone, { instancePath: instancePath + "/producerMilestone", parentData: data, parentDataProperty: "producerMilestone", rootData })) {
                        vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.selection !== undefined) {
                        const _errs8 = errors;
                        if (!validate193(data.selection, { instancePath: instancePath + "/selection", parentData: data, parentDataProperty: "selection", rootData })) {
                          vErrors = vErrors === null ? validate193.errors : vErrors.concat(validate193.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.selectionReceipt !== undefined) {
                          const _errs9 = errors;
                          if (!validate58(data.selectionReceipt, { instancePath: instancePath + "/selectionReceipt", parentData: data, parentDataProperty: "selectionReceipt", rootData })) {
                            vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.criteria !== undefined) {
                            let data8 = data.criteria;
                            const _errs10 = errors;
                            if (errors === _errs10) {
                              if (Array.isArray(data8)) {
                                if (data8.length > 2048) {
                                  validate180.errors = [{ instancePath: instancePath + "/criteria", schemaPath: "#/properties/criteria/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                  return false;
                                } else {
                                  var valid1 = true;
                                  const len0 = data8.length;
                                  for (let i0 = 0;i0 < len0; i0++) {
                                    let data9 = data8[i0];
                                    const _errs12 = errors;
                                    if (errors === _errs12) {
                                      if (data9 && typeof data9 == "object" && !Array.isArray(data9)) {
                                        let missing1;
                                        if (data9.criterionId === undefined && (missing1 = "criterionId") || data9.receipt === undefined && (missing1 = "receipt")) {
                                          validate180.errors = [{ instancePath: instancePath + "/criteria/" + i0, schemaPath: "#/properties/criteria/items/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                                          return false;
                                        } else {
                                          const _errs14 = errors;
                                          for (const key1 in data9) {
                                            if (!(key1 === "criterionId" || key1 === "receipt")) {
                                              validate180.errors = [{ instancePath: instancePath + "/criteria/" + i0, schemaPath: "#/properties/criteria/items/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                              return false;
                                              break;
                                            }
                                          }
                                          if (_errs14 === errors) {
                                            if (data9.criterionId !== undefined) {
                                              const _errs15 = errors;
                                              if (!validate24(data9.criterionId, { instancePath: instancePath + "/criteria/" + i0 + "/criterionId", parentData: data9, parentDataProperty: "criterionId", rootData })) {
                                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                errors = vErrors.length;
                                              }
                                              var valid2 = _errs15 === errors;
                                            } else {
                                              var valid2 = true;
                                            }
                                            if (valid2) {
                                              if (data9.receipt !== undefined) {
                                                const _errs16 = errors;
                                                if (!validate58(data9.receipt, { instancePath: instancePath + "/criteria/" + i0 + "/receipt", parentData: data9, parentDataProperty: "receipt", rootData })) {
                                                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid2 = _errs16 === errors;
                                              } else {
                                                var valid2 = true;
                                              }
                                            }
                                          }
                                        }
                                      } else {
                                        validate180.errors = [{ instancePath: instancePath + "/criteria/" + i0, schemaPath: "#/properties/criteria/items/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                        return false;
                                      }
                                    }
                                    var valid1 = _errs12 === errors;
                                    if (!valid1) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate180.errors = [{ instancePath: instancePath + "/criteria", schemaPath: "#/properties/criteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs10 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.receiverOwnerId !== undefined) {
                              const _errs17 = errors;
                              if (!validate24(data.receiverOwnerId, { instancePath: instancePath + "/receiverOwnerId", parentData: data, parentDataProperty: "receiverOwnerId", rootData })) {
                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                errors = vErrors.length;
                              }
                              var valid0 = _errs17 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.receiverUse !== undefined) {
                                const _errs18 = errors;
                                if (!validate58(data.receiverUse, { instancePath: instancePath + "/receiverUse", parentData: data, parentDataProperty: "receiverUse", rootData })) {
                                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                  errors = vErrors.length;
                                }
                                var valid0 = _errs18 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.producerSources !== undefined) {
                                  const _errs19 = errors;
                                  if (!validate213(data.producerSources, { instancePath: instancePath + "/producerSources", parentData: data, parentDataProperty: "producerSources", rootData })) {
                                    vErrors = vErrors === null ? validate213.errors : vErrors.concat(validate213.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid0 = _errs19 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.consumerSources !== undefined) {
                                    const _errs20 = errors;
                                    if (!validate213(data.consumerSources, { instancePath: instancePath + "/consumerSources", parentData: data, parentDataProperty: "consumerSources", rootData })) {
                                      vErrors = vErrors === null ? validate213.errors : vErrors.concat(validate213.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid0 = _errs20 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate180.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate180.errors = vErrors;
  return errors === 0;
}
function validate222(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.domains === undefined && (missing0 = "domains") || data.projects === undefined && (missing0 = "projects") || data.works === undefined && (missing0 = "works") || data.tasks === undefined && (missing0 = "tasks") || data.contracts === undefined && (missing0 = "contracts") || data.dependencies === undefined && (missing0 = "dependencies") || data.relations === undefined && (missing0 = "relations") || data.delegations === undefined && (missing0 = "delegations") || data.provenance === undefined && (missing0 = "provenance") || data.nativeCorrelations === undefined && (missing0 = "nativeCorrelations")) {
        validate222.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema7.properties, key0)) {
            validate222.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work.v3") {
              validate222.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.domains !== undefined) {
              let data1 = data.domains;
              const _errs3 = errors;
              if (errors === _errs3) {
                if (Array.isArray(data1)) {
                  if (data1.length > 2000) {
                    validate222.errors = [{ instancePath: instancePath + "/domains", schemaPath: "#/properties/domains/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                    return false;
                  } else {
                    var valid1 = true;
                    const len0 = data1.length;
                    for (let i0 = 0;i0 < len0; i0++) {
                      const _errs5 = errors;
                      if (!validate23(data1[i0], { instancePath: instancePath + "/domains/" + i0, parentData: data1, parentDataProperty: i0, rootData })) {
                        vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
                        errors = vErrors.length;
                      }
                      var valid1 = _errs5 === errors;
                      if (!valid1) {
                        break;
                      }
                    }
                  }
                } else {
                  validate222.errors = [{ instancePath: instancePath + "/domains", schemaPath: "#/properties/domains/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.projects !== undefined) {
                let data3 = data.projects;
                const _errs6 = errors;
                if (errors === _errs6) {
                  if (Array.isArray(data3)) {
                    if (data3.length > 2000) {
                      validate222.errors = [{ instancePath: instancePath + "/projects", schemaPath: "#/properties/projects/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                      return false;
                    } else {
                      var valid2 = true;
                      const len1 = data3.length;
                      for (let i1 = 0;i1 < len1; i1++) {
                        const _errs8 = errors;
                        if (!validate29(data3[i1], { instancePath: instancePath + "/projects/" + i1, parentData: data3, parentDataProperty: i1, rootData })) {
                          vErrors = vErrors === null ? validate29.errors : vErrors.concat(validate29.errors);
                          errors = vErrors.length;
                        }
                        var valid2 = _errs8 === errors;
                        if (!valid2) {
                          break;
                        }
                      }
                    }
                  } else {
                    validate222.errors = [{ instancePath: instancePath + "/projects", schemaPath: "#/properties/projects/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                    return false;
                  }
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.works !== undefined) {
                  let data5 = data.works;
                  const _errs9 = errors;
                  if (errors === _errs9) {
                    if (Array.isArray(data5)) {
                      if (data5.length > 2000) {
                        validate222.errors = [{ instancePath: instancePath + "/works", schemaPath: "#/properties/works/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                        return false;
                      } else {
                        var valid3 = true;
                        const len2 = data5.length;
                        for (let i2 = 0;i2 < len2; i2++) {
                          const _errs11 = errors;
                          if (!validate34(data5[i2], { instancePath: instancePath + "/works/" + i2, parentData: data5, parentDataProperty: i2, rootData })) {
                            vErrors = vErrors === null ? validate34.errors : vErrors.concat(validate34.errors);
                            errors = vErrors.length;
                          }
                          var valid3 = _errs11 === errors;
                          if (!valid3) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate222.errors = [{ instancePath: instancePath + "/works", schemaPath: "#/properties/works/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs9 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.tasks !== undefined) {
                    let data7 = data.tasks;
                    const _errs12 = errors;
                    if (errors === _errs12) {
                      if (Array.isArray(data7)) {
                        if (data7.length > 2000) {
                          validate222.errors = [{ instancePath: instancePath + "/tasks", schemaPath: "#/properties/tasks/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                          return false;
                        } else {
                          var valid4 = true;
                          const len3 = data7.length;
                          for (let i3 = 0;i3 < len3; i3++) {
                            const _errs14 = errors;
                            if (!validate63(data7[i3], { instancePath: instancePath + "/tasks/" + i3, parentData: data7, parentDataProperty: i3, rootData })) {
                              vErrors = vErrors === null ? validate63.errors : vErrors.concat(validate63.errors);
                              errors = vErrors.length;
                            }
                            var valid4 = _errs14 === errors;
                            if (!valid4) {
                              break;
                            }
                          }
                        }
                      } else {
                        validate222.errors = [{ instancePath: instancePath + "/tasks", schemaPath: "#/properties/tasks/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                        return false;
                      }
                    }
                    var valid0 = _errs12 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.contracts !== undefined) {
                      let data9 = data.contracts;
                      const _errs15 = errors;
                      if (errors === _errs15) {
                        if (Array.isArray(data9)) {
                          if (data9.length > 1e4) {
                            validate222.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                            return false;
                          } else {
                            var valid5 = true;
                            const len4 = data9.length;
                            for (let i4 = 0;i4 < len4; i4++) {
                              const _errs17 = errors;
                              if (!validate86(data9[i4], { instancePath: instancePath + "/contracts/" + i4, parentData: data9, parentDataProperty: i4, rootData })) {
                                vErrors = vErrors === null ? validate86.errors : vErrors.concat(validate86.errors);
                                errors = vErrors.length;
                              }
                              var valid5 = _errs17 === errors;
                              if (!valid5) {
                                break;
                              }
                            }
                          }
                        } else {
                          validate222.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                          return false;
                        }
                      }
                      var valid0 = _errs15 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.dependencies !== undefined) {
                        let data11 = data.dependencies;
                        const _errs18 = errors;
                        if (errors === _errs18) {
                          if (Array.isArray(data11)) {
                            if (data11.length > 1e4) {
                              validate222.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                              return false;
                            } else {
                              var valid6 = true;
                              const len5 = data11.length;
                              for (let i5 = 0;i5 < len5; i5++) {
                                const _errs20 = errors;
                                if (!validate126(data11[i5], { instancePath: instancePath + "/dependencies/" + i5, parentData: data11, parentDataProperty: i5, rootData })) {
                                  vErrors = vErrors === null ? validate126.errors : vErrors.concat(validate126.errors);
                                  errors = vErrors.length;
                                }
                                var valid6 = _errs20 === errors;
                                if (!valid6) {
                                  break;
                                }
                              }
                            }
                          } else {
                            validate222.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                            return false;
                          }
                        }
                        var valid0 = _errs18 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.relations !== undefined) {
                          let data13 = data.relations;
                          const _errs21 = errors;
                          if (errors === _errs21) {
                            if (Array.isArray(data13)) {
                              if (data13.length > 1e4) {
                                validate222.errors = [{ instancePath: instancePath + "/relations", schemaPath: "#/properties/relations/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                return false;
                              } else {
                                var valid7 = true;
                                const len6 = data13.length;
                                for (let i6 = 0;i6 < len6; i6++) {
                                  const _errs23 = errors;
                                  if (!validate139(data13[i6], { instancePath: instancePath + "/relations/" + i6, parentData: data13, parentDataProperty: i6, rootData })) {
                                    vErrors = vErrors === null ? validate139.errors : vErrors.concat(validate139.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid7 = _errs23 === errors;
                                  if (!valid7) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate222.errors = [{ instancePath: instancePath + "/relations", schemaPath: "#/properties/relations/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs21 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.delegations !== undefined) {
                            let data15 = data.delegations;
                            const _errs24 = errors;
                            if (errors === _errs24) {
                              if (Array.isArray(data15)) {
                                if (data15.length > 1e4) {
                                  validate222.errors = [{ instancePath: instancePath + "/delegations", schemaPath: "#/properties/delegations/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                  return false;
                                } else {
                                  var valid8 = true;
                                  const len7 = data15.length;
                                  for (let i7 = 0;i7 < len7; i7++) {
                                    const _errs26 = errors;
                                    if (!validate152(data15[i7], { instancePath: instancePath + "/delegations/" + i7, parentData: data15, parentDataProperty: i7, rootData })) {
                                      vErrors = vErrors === null ? validate152.errors : vErrors.concat(validate152.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid8 = _errs26 === errors;
                                    if (!valid8) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate222.errors = [{ instancePath: instancePath + "/delegations", schemaPath: "#/properties/delegations/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs24 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.provenance !== undefined) {
                              let data17 = data.provenance;
                              const _errs27 = errors;
                              if (errors === _errs27) {
                                if (Array.isArray(data17)) {
                                  if (data17.length > 1e4) {
                                    validate222.errors = [{ instancePath: instancePath + "/provenance", schemaPath: "#/properties/provenance/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                    return false;
                                  } else {
                                    var valid9 = true;
                                    const len8 = data17.length;
                                    for (let i8 = 0;i8 < len8; i8++) {
                                      const _errs29 = errors;
                                      if (!validate157(data17[i8], { instancePath: instancePath + "/provenance/" + i8, parentData: data17, parentDataProperty: i8, rootData })) {
                                        vErrors = vErrors === null ? validate157.errors : vErrors.concat(validate157.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid9 = _errs29 === errors;
                                      if (!valid9) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate222.errors = [{ instancePath: instancePath + "/provenance", schemaPath: "#/properties/provenance/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs27 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.nativeCorrelations !== undefined) {
                                let data19 = data.nativeCorrelations;
                                const _errs30 = errors;
                                if (errors === _errs30) {
                                  if (Array.isArray(data19)) {
                                    if (data19.length > 1e4) {
                                      validate222.errors = [{ instancePath: instancePath + "/nativeCorrelations", schemaPath: "#/properties/nativeCorrelations/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                      return false;
                                    } else {
                                      var valid10 = true;
                                      const len9 = data19.length;
                                      for (let i9 = 0;i9 < len9; i9++) {
                                        const _errs32 = errors;
                                        if (!validate161(data19[i9], { instancePath: instancePath + "/nativeCorrelations/" + i9, parentData: data19, parentDataProperty: i9, rootData })) {
                                          vErrors = vErrors === null ? validate161.errors : vErrors.concat(validate161.errors);
                                          errors = vErrors.length;
                                        }
                                        var valid10 = _errs32 === errors;
                                        if (!valid10) {
                                          break;
                                        }
                                      }
                                    }
                                  } else {
                                    validate222.errors = [{ instancePath: instancePath + "/nativeCorrelations", schemaPath: "#/properties/nativeCorrelations/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs30 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.aliases !== undefined) {
                                  let data21 = data.aliases;
                                  const _errs33 = errors;
                                  if (errors === _errs33) {
                                    if (Array.isArray(data21)) {
                                      if (data21.length > 2048) {
                                        validate222.errors = [{ instancePath: instancePath + "/aliases", schemaPath: "#/properties/aliases/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                        return false;
                                      } else {
                                        var valid11 = true;
                                        const len10 = data21.length;
                                        for (let i10 = 0;i10 < len10; i10++) {
                                          const _errs35 = errors;
                                          if (!validate171(data21[i10], { instancePath: instancePath + "/aliases/" + i10, parentData: data21, parentDataProperty: i10, rootData })) {
                                            vErrors = vErrors === null ? validate171.errors : vErrors.concat(validate171.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid11 = _errs35 === errors;
                                          if (!valid11) {
                                            break;
                                          }
                                        }
                                      }
                                    } else {
                                      validate222.errors = [{ instancePath: instancePath + "/aliases", schemaPath: "#/properties/aliases/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs33 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.externalArtifacts !== undefined) {
                                    let data23 = data.externalArtifacts;
                                    const _errs36 = errors;
                                    if (errors === _errs36) {
                                      if (Array.isArray(data23)) {
                                        if (data23.length > 2048) {
                                          validate222.errors = [{ instancePath: instancePath + "/externalArtifacts", schemaPath: "#/properties/externalArtifacts/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                          return false;
                                        } else {
                                          var valid12 = true;
                                          const len11 = data23.length;
                                          for (let i11 = 0;i11 < len11; i11++) {
                                            const _errs38 = errors;
                                            if (!validate180(data23[i11], { instancePath: instancePath + "/externalArtifacts/" + i11, parentData: data23, parentDataProperty: i11, rootData })) {
                                              vErrors = vErrors === null ? validate180.errors : vErrors.concat(validate180.errors);
                                              errors = vErrors.length;
                                            }
                                            var valid12 = _errs38 === errors;
                                            if (!valid12) {
                                              break;
                                            }
                                          }
                                        }
                                      } else {
                                        validate222.errors = [{ instancePath: instancePath + "/externalArtifacts", schemaPath: "#/properties/externalArtifacts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                        return false;
                                      }
                                    }
                                    var valid0 = _errs36 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate222.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate222.errors = vErrors;
  return errors === 0;
}
var validateWork = validate235;
function validate235(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.title === undefined && (missing0 = "title") || data.leadActorId === undefined && (missing0 = "leadActorId") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts") || data.status === undefined && (missing0 = "status")) {
        validate235.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId" || key0 === "title" || key0 === "leadActorId" || key0 === "revision" || key0 === "contracts" || key0 === "status" || key0 === "legacySource")) {
            validate235.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.title !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.title, { instancePath: instancePath + "/title", parentData: data, parentDataProperty: "title", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.leadActorId !== undefined) {
                  let data3 = data.leadActorId;
                  const _errs5 = errors;
                  const _errs6 = errors;
                  let valid1 = false;
                  const _errs7 = errors;
                  if (!validate24(data3, { instancePath: instancePath + "/leadActorId", parentData: data, parentDataProperty: "leadActorId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var _valid0 = _errs7 === errors;
                  valid1 = valid1 || _valid0;
                  if (!valid1) {
                    const _errs8 = errors;
                    if (data3 !== null) {
                      const err0 = { instancePath: instancePath + "/leadActorId", schemaPath: "#/properties/leadActorId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                      if (vErrors === null) {
                        vErrors = [err0];
                      } else {
                        vErrors.push(err0);
                      }
                      errors++;
                    }
                    var _valid0 = _errs8 === errors;
                    valid1 = valid1 || _valid0;
                  }
                  if (!valid1) {
                    const err1 = { instancePath: instancePath + "/leadActorId", schemaPath: "#/properties/leadActorId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                    if (vErrors === null) {
                      vErrors = [err1];
                    } else {
                      vErrors.push(err1);
                    }
                    errors++;
                    validate235.errors = vErrors;
                    return false;
                  } else {
                    errors = _errs6;
                    if (vErrors !== null) {
                      if (_errs6) {
                        vErrors.length = _errs6;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.revision !== undefined) {
                    const _errs10 = errors;
                    if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                      vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.contracts !== undefined) {
                      const _errs11 = errors;
                      if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                        vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.status !== undefined) {
                        let data6 = data.status;
                        const _errs12 = errors;
                        if (!(data6 === "proposed" || data6 === "admitted" || data6 === "active" || data6 === "verified" || data6 === "accepted" || data6 === "cancelled")) {
                          validate235.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema11.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.legacySource !== undefined) {
                          const _errs13 = errors;
                          if (!validate58(data.legacySource, { instancePath: instancePath + "/legacySource", parentData: data, parentDataProperty: "legacySource", rootData })) {
                            vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs13 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate235.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate235.errors = vErrors;
  return errors === 0;
}
var validateTask = validate243;
function validate243(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.taskId === undefined && (missing0 = "taskId") || data.title === undefined && (missing0 = "title") || data.assigneeActorId === undefined && (missing0 = "assigneeActorId") || data.revision === undefined && (missing0 = "revision") || data.contractOverrides === undefined && (missing0 = "contractOverrides") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.resources === undefined && (missing0 = "resources") || data.status === undefined && (missing0 = "status")) {
        validate243.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema18.properties, key0)) {
            validate243.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.taskId, { instancePath: instancePath + "/taskId", parentData: data, parentDataProperty: "taskId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.title !== undefined) {
                  const _errs5 = errors;
                  if (!validate24(data.title, { instancePath: instancePath + "/title", parentData: data, parentDataProperty: "title", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.parentTaskId !== undefined) {
                    const _errs6 = errors;
                    if (!validate24(data.parentTaskId, { instancePath: instancePath + "/parentTaskId", parentData: data, parentDataProperty: "parentTaskId", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.assigneeActorId !== undefined) {
                      let data5 = data.assigneeActorId;
                      const _errs7 = errors;
                      const _errs8 = errors;
                      let valid1 = false;
                      const _errs9 = errors;
                      if (!validate24(data5, { instancePath: instancePath + "/assigneeActorId", parentData: data, parentDataProperty: "assigneeActorId", rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var _valid0 = _errs9 === errors;
                      valid1 = valid1 || _valid0;
                      if (!valid1) {
                        const _errs10 = errors;
                        if (data5 !== null) {
                          const err0 = { instancePath: instancePath + "/assigneeActorId", schemaPath: "#/properties/assigneeActorId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                          if (vErrors === null) {
                            vErrors = [err0];
                          } else {
                            vErrors.push(err0);
                          }
                          errors++;
                        }
                        var _valid0 = _errs10 === errors;
                        valid1 = valid1 || _valid0;
                      }
                      if (!valid1) {
                        const err1 = { instancePath: instancePath + "/assigneeActorId", schemaPath: "#/properties/assigneeActorId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                        if (vErrors === null) {
                          vErrors = [err1];
                        } else {
                          vErrors.push(err1);
                        }
                        errors++;
                        validate243.errors = vErrors;
                        return false;
                      } else {
                        errors = _errs8;
                        if (vErrors !== null) {
                          if (_errs8) {
                            vErrors.length = _errs8;
                          } else {
                            vErrors = null;
                          }
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.revision !== undefined) {
                        const _errs12 = errors;
                        if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                          vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.contractOverrides !== undefined) {
                          const _errs13 = errors;
                          if (!validate71(data.contractOverrides, { instancePath: instancePath + "/contractOverrides", parentData: data, parentDataProperty: "contractOverrides", rootData })) {
                            vErrors = vErrors === null ? validate71.errors : vErrors.concat(validate71.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs13 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.acceptanceCriteria !== undefined) {
                            let data8 = data.acceptanceCriteria;
                            const _errs14 = errors;
                            if (errors === _errs14) {
                              if (Array.isArray(data8)) {
                                if (data8.length > 2000) {
                                  validate243.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len0 = data8.length;
                                  for (let i0 = 0;i0 < len0; i0++) {
                                    const _errs16 = errors;
                                    if (!validate24(data8[i0], { instancePath: instancePath + "/acceptanceCriteria/" + i0, parentData: data8, parentDataProperty: i0, rootData })) {
                                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs16 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate243.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs14 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.resources !== undefined) {
                              let data10 = data.resources;
                              const _errs17 = errors;
                              if (errors === _errs17) {
                                if (Array.isArray(data10)) {
                                  if (data10.length > 2000) {
                                    validate243.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid3 = true;
                                    const len1 = data10.length;
                                    for (let i1 = 0;i1 < len1; i1++) {
                                      const _errs19 = errors;
                                      if (!validate77(data10[i1], { instancePath: instancePath + "/resources/" + i1, parentData: data10, parentDataProperty: i1, rootData })) {
                                        vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid3 = _errs19 === errors;
                                      if (!valid3) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate243.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs17 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.status !== undefined) {
                                let data12 = data.status;
                                const _errs20 = errors;
                                if (!(data12 === "planned" || data12 === "ready" || data12 === "running" || data12 === "blocked" || data12 === "verified" || data12 === "accepted" || data12 === "cancelled")) {
                                  validate243.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema18.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid0 = _errs20 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.legacyBinding !== undefined) {
                                  const _errs21 = errors;
                                  if (!validate80(data.legacyBinding, { instancePath: instancePath + "/legacyBinding", parentData: data, parentDataProperty: "legacyBinding", rootData })) {
                                    vErrors = vErrors === null ? validate80.errors : vErrors.concat(validate80.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid0 = _errs21 === errors;
                                } else {
                                  var valid0 = true;
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate243.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate243.errors = vErrors;
  return errors === 0;
}
var validateRun = validate255;
var schema48 = { type: "object", properties: { runId: { $ref: "#/$defs/Id" }, task: { $ref: "#/$defs/TaskRef" }, attempt: { $ref: "#/$defs/Revision" }, status: { enum: ["reserved", "started", "succeeded", "failed", "cancelled", "uncertain"] }, effectiveContractDigest: { $ref: "#/$defs/Digest" }, sourceSnapshotId: { $ref: "#/$defs/Id" }, parentAuthorityRef: { $ref: "#/$defs/Id" }, nativeBindings: { type: "array", items: { $ref: "#/$defs/NativeBinding" }, maxItems: 2000 }, effectRefs: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2000 } }, required: ["runId", "task", "attempt", "status", "effectiveContractDigest", "sourceSnapshotId", "parentAuthorityRef", "nativeBindings", "effectRefs"], additionalProperties: false };
function validate255(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.runId === undefined && (missing0 = "runId") || data.task === undefined && (missing0 = "task") || data.attempt === undefined && (missing0 = "attempt") || data.status === undefined && (missing0 = "status") || data.effectiveContractDigest === undefined && (missing0 = "effectiveContractDigest") || data.sourceSnapshotId === undefined && (missing0 = "sourceSnapshotId") || data.parentAuthorityRef === undefined && (missing0 = "parentAuthorityRef") || data.nativeBindings === undefined && (missing0 = "nativeBindings") || data.effectRefs === undefined && (missing0 = "effectRefs")) {
        validate255.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema48.properties, key0)) {
            validate255.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.runId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.runId, { instancePath: instancePath + "/runId", parentData: data, parentDataProperty: "runId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs3 = errors;
              if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.attempt !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.attempt, { instancePath: instancePath + "/attempt", parentData: data, parentDataProperty: "attempt", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.status !== undefined) {
                  let data3 = data.status;
                  const _errs5 = errors;
                  if (!(data3 === "reserved" || data3 === "started" || data3 === "succeeded" || data3 === "failed" || data3 === "cancelled" || data3 === "uncertain")) {
                    validate255.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema48.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.effectiveContractDigest !== undefined) {
                    const _errs6 = errors;
                    if (!validate52(data.effectiveContractDigest, { instancePath: instancePath + "/effectiveContractDigest", parentData: data, parentDataProperty: "effectiveContractDigest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.sourceSnapshotId !== undefined) {
                      const _errs7 = errors;
                      if (!validate24(data.sourceSnapshotId, { instancePath: instancePath + "/sourceSnapshotId", parentData: data, parentDataProperty: "sourceSnapshotId", rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.parentAuthorityRef !== undefined) {
                        const _errs8 = errors;
                        if (!validate24(data.parentAuthorityRef, { instancePath: instancePath + "/parentAuthorityRef", parentData: data, parentDataProperty: "parentAuthorityRef", rootData })) {
                          vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.nativeBindings !== undefined) {
                          let data7 = data.nativeBindings;
                          const _errs9 = errors;
                          if (errors === _errs9) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate255.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid1 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs11 = errors;
                                  if (!validate163(data7[i0], { instancePath: instancePath + "/nativeBindings/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate163.errors : vErrors.concat(validate163.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid1 = _errs11 === errors;
                                  if (!valid1) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate255.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.effectRefs !== undefined) {
                            let data9 = data.effectRefs;
                            const _errs12 = errors;
                            if (errors === _errs12) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate255.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs14 = errors;
                                    if (!validate24(data9[i1], { instancePath: instancePath + "/effectRefs/" + i1, parentData: data9, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs14 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate255.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs12 === errors;
                          } else {
                            var valid0 = true;
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate255.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate255.errors = vErrors;
  return errors === 0;
}
var validateContract = validate264;
function validate264(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (errors === _errs1) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.owner === undefined && (missing0 = "owner") || data.revision === undefined && (missing0 = "revision") || data.kind === undefined && (missing0 = "kind") || data.digest === undefined && (missing0 = "digest") || data.sourceRefs === undefined && (missing0 = "sourceRefs") || data.body === undefined && (missing0 = "body")) {
        const err0 = { instancePath, schemaPath: "#/oneOf/0/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        const _errs3 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "owner" || key0 === "revision" || key0 === "kind" || key0 === "digest" || key0 === "sourceRefs" || key0 === "body")) {
            const err1 = { instancePath, schemaPath: "#/oneOf/0/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
            break;
          }
        }
        if (_errs3 === errors) {
          if (data.id !== undefined) {
            const _errs4 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.owner !== undefined) {
              const _errs5 = errors;
              if (!validate44(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.revision !== undefined) {
                const _errs6 = errors;
                if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid1 = _errs6 === errors;
              } else {
                var valid1 = true;
              }
              if (valid1) {
                if (data.kind !== undefined) {
                  const _errs7 = errors;
                  if (data.kind !== "issue") {
                    const err2 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/0/properties/kind/const", keyword: "const", params: { allowedValue: "issue" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err2];
                    } else {
                      vErrors.push(err2);
                    }
                    errors++;
                  }
                  var valid1 = _errs7 === errors;
                } else {
                  var valid1 = true;
                }
                if (valid1) {
                  if (data.digest !== undefined) {
                    const _errs8 = errors;
                    if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid1 = _errs8 === errors;
                  } else {
                    var valid1 = true;
                  }
                  if (valid1) {
                    if (data.sourceRefs !== undefined) {
                      let data5 = data.sourceRefs;
                      const _errs9 = errors;
                      if (errors === _errs9) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2000) {
                            const err3 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err3];
                            } else {
                              vErrors.push(err3);
                            }
                            errors++;
                          } else {
                            var valid2 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs11 = errors;
                              if (!validate91(data5[i0], { instancePath: instancePath + "/sourceRefs/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                                errors = vErrors.length;
                              }
                              var valid2 = _errs11 === errors;
                              if (!valid2) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err4 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err4];
                          } else {
                            vErrors.push(err4);
                          }
                          errors++;
                        }
                      }
                      var valid1 = _errs9 === errors;
                    } else {
                      var valid1 = true;
                    }
                    if (valid1) {
                      if (data.body !== undefined) {
                        const _errs12 = errors;
                        if (!validate96(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate96.errors : vErrors.concat(validate96.errors);
                          errors = vErrors.length;
                        }
                        var valid1 = _errs12 === errors;
                      } else {
                        var valid1 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      const err5 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err5];
      } else {
        vErrors.push(err5);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs13 = errors;
  if (errors === _errs13) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.id === undefined && (missing1 = "id") || data.owner === undefined && (missing1 = "owner") || data.revision === undefined && (missing1 = "revision") || data.kind === undefined && (missing1 = "kind") || data.digest === undefined && (missing1 = "digest") || data.sourceRefs === undefined && (missing1 = "sourceRefs") || data.body === undefined && (missing1 = "body")) {
        const err6 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err6];
        } else {
          vErrors.push(err6);
        }
        errors++;
      } else {
        const _errs15 = errors;
        for (const key1 in data) {
          if (!(key1 === "id" || key1 === "owner" || key1 === "revision" || key1 === "kind" || key1 === "digest" || key1 === "sourceRefs" || key1 === "body")) {
            const err7 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err7];
            } else {
              vErrors.push(err7);
            }
            errors++;
            break;
          }
        }
        if (_errs15 === errors) {
          if (data.id !== undefined) {
            const _errs16 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid3 = _errs16 === errors;
          } else {
            var valid3 = true;
          }
          if (valid3) {
            if (data.owner !== undefined) {
              const _errs17 = errors;
              if (!validate44(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
                errors = vErrors.length;
              }
              var valid3 = _errs17 === errors;
            } else {
              var valid3 = true;
            }
            if (valid3) {
              if (data.revision !== undefined) {
                const _errs18 = errors;
                if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid3 = _errs18 === errors;
              } else {
                var valid3 = true;
              }
              if (valid3) {
                if (data.kind !== undefined) {
                  const _errs19 = errors;
                  if (data.kind !== "spec") {
                    const err8 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "spec" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err8];
                    } else {
                      vErrors.push(err8);
                    }
                    errors++;
                  }
                  var valid3 = _errs19 === errors;
                } else {
                  var valid3 = true;
                }
                if (valid3) {
                  if (data.digest !== undefined) {
                    const _errs20 = errors;
                    if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid3 = _errs20 === errors;
                  } else {
                    var valid3 = true;
                  }
                  if (valid3) {
                    if (data.sourceRefs !== undefined) {
                      let data13 = data.sourceRefs;
                      const _errs21 = errors;
                      if (errors === _errs21) {
                        if (Array.isArray(data13)) {
                          if (data13.length > 2000) {
                            const err9 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err9];
                            } else {
                              vErrors.push(err9);
                            }
                            errors++;
                          } else {
                            var valid4 = true;
                            const len1 = data13.length;
                            for (let i1 = 0;i1 < len1; i1++) {
                              const _errs23 = errors;
                              if (!validate91(data13[i1], { instancePath: instancePath + "/sourceRefs/" + i1, parentData: data13, parentDataProperty: i1, rootData })) {
                                vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                                errors = vErrors.length;
                              }
                              var valid4 = _errs23 === errors;
                              if (!valid4) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err10 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err10];
                          } else {
                            vErrors.push(err10);
                          }
                          errors++;
                        }
                      }
                      var valid3 = _errs21 === errors;
                    } else {
                      var valid3 = true;
                    }
                    if (valid3) {
                      if (data.body !== undefined) {
                        const _errs24 = errors;
                        if (!validate106(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate106.errors : vErrors.concat(validate106.errors);
                          errors = vErrors.length;
                        }
                        var valid3 = _errs24 === errors;
                      } else {
                        var valid3 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      const err11 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err11];
      } else {
        vErrors.push(err11);
      }
      errors++;
    }
  }
  var _valid0 = _errs13 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
    const _errs25 = errors;
    if (errors === _errs25) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.id === undefined && (missing2 = "id") || data.owner === undefined && (missing2 = "owner") || data.revision === undefined && (missing2 = "revision") || data.kind === undefined && (missing2 = "kind") || data.digest === undefined && (missing2 = "digest") || data.sourceRefs === undefined && (missing2 = "sourceRefs") || data.body === undefined && (missing2 = "body")) {
          const err12 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err12];
          } else {
            vErrors.push(err12);
          }
          errors++;
        } else {
          const _errs27 = errors;
          for (const key2 in data) {
            if (!(key2 === "id" || key2 === "owner" || key2 === "revision" || key2 === "kind" || key2 === "digest" || key2 === "sourceRefs" || key2 === "body")) {
              const err13 = { instancePath, schemaPath: "#/oneOf/2/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" };
              if (vErrors === null) {
                vErrors = [err13];
              } else {
                vErrors.push(err13);
              }
              errors++;
              break;
            }
          }
          if (_errs27 === errors) {
            if (data.id !== undefined) {
              const _errs28 = errors;
              if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid5 = _errs28 === errors;
            } else {
              var valid5 = true;
            }
            if (valid5) {
              if (data.owner !== undefined) {
                const _errs29 = errors;
                if (!validate44(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                  vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
                  errors = vErrors.length;
                }
                var valid5 = _errs29 === errors;
              } else {
                var valid5 = true;
              }
              if (valid5) {
                if (data.revision !== undefined) {
                  const _errs30 = errors;
                  if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                    vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                    errors = vErrors.length;
                  }
                  var valid5 = _errs30 === errors;
                } else {
                  var valid5 = true;
                }
                if (valid5) {
                  if (data.kind !== undefined) {
                    const _errs31 = errors;
                    if (data.kind !== "plan") {
                      const err14 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/2/properties/kind/const", keyword: "const", params: { allowedValue: "plan" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err14];
                      } else {
                        vErrors.push(err14);
                      }
                      errors++;
                    }
                    var valid5 = _errs31 === errors;
                  } else {
                    var valid5 = true;
                  }
                  if (valid5) {
                    if (data.digest !== undefined) {
                      const _errs32 = errors;
                      if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                        vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                        errors = vErrors.length;
                      }
                      var valid5 = _errs32 === errors;
                    } else {
                      var valid5 = true;
                    }
                    if (valid5) {
                      if (data.sourceRefs !== undefined) {
                        let data21 = data.sourceRefs;
                        const _errs33 = errors;
                        if (errors === _errs33) {
                          if (Array.isArray(data21)) {
                            if (data21.length > 2000) {
                              const err15 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                              if (vErrors === null) {
                                vErrors = [err15];
                              } else {
                                vErrors.push(err15);
                              }
                              errors++;
                            } else {
                              var valid6 = true;
                              const len2 = data21.length;
                              for (let i2 = 0;i2 < len2; i2++) {
                                const _errs35 = errors;
                                if (!validate91(data21[i2], { instancePath: instancePath + "/sourceRefs/" + i2, parentData: data21, parentDataProperty: i2, rootData })) {
                                  vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
                                  errors = vErrors.length;
                                }
                                var valid6 = _errs35 === errors;
                                if (!valid6) {
                                  break;
                                }
                              }
                            }
                          } else {
                            const err16 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                            if (vErrors === null) {
                              vErrors = [err16];
                            } else {
                              vErrors.push(err16);
                            }
                            errors++;
                          }
                        }
                        var valid5 = _errs33 === errors;
                      } else {
                        var valid5 = true;
                      }
                      if (valid5) {
                        if (data.body !== undefined) {
                          const _errs36 = errors;
                          if (!validate120(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                            vErrors = vErrors === null ? validate120.errors : vErrors.concat(validate120.errors);
                            errors = vErrors.length;
                          }
                          var valid5 = _errs36 === errors;
                        } else {
                          var valid5 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      } else {
        const err17 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err17];
        } else {
          vErrors.push(err17);
        }
        errors++;
      }
    }
    var _valid0 = _errs25 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
      }
    }
  }
  if (!valid0) {
    const err18 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err18];
    } else {
      vErrors.push(err18);
    }
    errors++;
    validate264.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate264.errors = vErrors;
  return errors === 0;
}
var validateCreateRequest = validate283;
var schema50 = { type: "object", properties: { schema: { const: "common.git-work-create.v3" }, ownerId: { $ref: "#/$defs/Id" }, createRequestId: { $ref: "#/$defs/Id" }, kind: { enum: ["work", "task", "run"] }, scope: { $ref: "#/$defs/Scope" }, record: { $ref: "#/$defs/Json" }, contracts: { type: "array", items: { $ref: "#/$defs/Json" }, maxItems: 100 } }, required: ["schema", "ownerId", "createRequestId", "kind", "scope", "record", "contracts"], additionalProperties: false };
function validate283(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.createRequestId === undefined && (missing0 = "createRequestId") || data.kind === undefined && (missing0 = "kind") || data.scope === undefined && (missing0 = "scope") || data.record === undefined && (missing0 = "record") || data.contracts === undefined && (missing0 = "contracts")) {
        validate283.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "ownerId" || key0 === "createRequestId" || key0 === "kind" || key0 === "scope" || key0 === "record" || key0 === "contracts")) {
            validate283.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-create.v3") {
              validate283.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-create.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.ownerId, { instancePath: instancePath + "/ownerId", parentData: data, parentDataProperty: "ownerId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.createRequestId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.createRequestId, { instancePath: instancePath + "/createRequestId", parentData: data, parentDataProperty: "createRequestId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.kind !== undefined) {
                  let data3 = data.kind;
                  const _errs5 = errors;
                  if (!(data3 === "work" || data3 === "task" || data3 === "run")) {
                    validate283.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema50.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.scope !== undefined) {
                    const _errs6 = errors;
                    if (!validate140(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData })) {
                      vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.record !== undefined) {
                      const _errs7 = errors;
                      if (!validate97(data.record, { instancePath: instancePath + "/record", parentData: data, parentDataProperty: "record", rootData })) {
                        vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.contracts !== undefined) {
                        let data6 = data.contracts;
                        const _errs8 = errors;
                        if (errors === _errs8) {
                          if (Array.isArray(data6)) {
                            if (data6.length > 100) {
                              validate283.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                              return false;
                            } else {
                              var valid1 = true;
                              const len0 = data6.length;
                              for (let i0 = 0;i0 < len0; i0++) {
                                const _errs10 = errors;
                                if (!validate97(data6[i0], { instancePath: instancePath + "/contracts/" + i0, parentData: data6, parentDataProperty: i0, rootData })) {
                                  vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
                                  errors = vErrors.length;
                                }
                                var valid1 = _errs10 === errors;
                                if (!valid1) {
                                  break;
                                }
                              }
                            }
                          } else {
                            validate283.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                            return false;
                          }
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate283.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate283.errors = vErrors;
  return errors === 0;
}
var validateEffectiveContract = validate289;
var schema51 = { type: "object", properties: { schema: { const: "common.git-work-effective.v3" }, task: { $ref: "#/$defs/TaskRef" }, workRevision: { $ref: "#/$defs/Revision" }, taskRevision: { $ref: "#/$defs/Revision" }, workContracts: { $ref: "#/$defs/ContractSet" }, taskOverrides: { $ref: "#/$defs/ContractOverrides" }, contracts: { $ref: "#/$defs/ContractSet" }, parentTaskChain: { type: "array", items: { $ref: "#/$defs/Ancestor" }, maxItems: 2000 }, taskResources: { type: "array", items: { $ref: "#/$defs/ResourceClaim" }, maxItems: 2000 }, acceptanceCriteria: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2000 }, dependencies: { type: "array", items: { $ref: "#/$defs/Dependency" }, maxItems: 1e4 }, dependencyTasks: { type: "array", items: { $ref: "#/$defs/DependencyTask" }, maxItems: 2000 }, digest: { $ref: "#/$defs/Digest" }, legacyBinding: { $ref: "#/$defs/LegacyTaskBinding" }, holdReasons: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2048 }, externalArtifacts: { type: "array", items: { $ref: "#/$defs/SelectedExternalArtifact" }, maxItems: 2048 } }, required: ["schema", "task", "workRevision", "taskRevision", "workContracts", "taskOverrides", "contracts", "parentTaskChain", "taskResources", "acceptanceCriteria", "dependencies", "dependencyTasks", "digest", "holdReasons", "externalArtifacts"], additionalProperties: false };
function validate296(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts")) {
        validate296.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "revision" || key0 === "contracts")) {
            validate296.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.revision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.contracts !== undefined) {
                const _errs4 = errors;
                if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                  vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate296.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate296.errors = vErrors;
  return errors === 0;
}
function validate304(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts") || data.resources === undefined && (missing0 = "resources") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria")) {
        validate304.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "revision" || key0 === "contracts" || key0 === "resources" || key0 === "acceptanceCriteria")) {
            validate304.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.revision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.contracts !== undefined) {
                const _errs4 = errors;
                if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                  vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.resources !== undefined) {
                  let data3 = data.resources;
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 2000) {
                        validate304.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate77(data3[i0], { instancePath: instancePath + "/resources/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate304.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.acceptanceCriteria !== undefined) {
                    let data5 = data.acceptanceCriteria;
                    const _errs8 = errors;
                    if (errors === _errs8) {
                      if (Array.isArray(data5)) {
                        if (data5.length > 2000) {
                          validate304.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                          return false;
                        } else {
                          var valid2 = true;
                          const len1 = data5.length;
                          for (let i1 = 0;i1 < len1; i1++) {
                            const _errs10 = errors;
                            if (!validate24(data5[i1], { instancePath: instancePath + "/acceptanceCriteria/" + i1, parentData: data5, parentDataProperty: i1, rootData })) {
                              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                              errors = vErrors.length;
                            }
                            var valid2 = _errs10 === errors;
                            if (!valid2) {
                              break;
                            }
                          }
                        }
                      } else {
                        validate304.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                        return false;
                      }
                    }
                    var valid0 = _errs8 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate304.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate304.errors = vErrors;
  return errors === 0;
}
function validate289(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.taskOverrides === undefined && (missing0 = "taskOverrides") || data.contracts === undefined && (missing0 = "contracts") || data.parentTaskChain === undefined && (missing0 = "parentTaskChain") || data.taskResources === undefined && (missing0 = "taskResources") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.dependencies === undefined && (missing0 = "dependencies") || data.dependencyTasks === undefined && (missing0 = "dependencyTasks") || data.digest === undefined && (missing0 = "digest") || data.holdReasons === undefined && (missing0 = "holdReasons") || data.externalArtifacts === undefined && (missing0 = "externalArtifacts")) {
        validate289.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema51.properties, key0)) {
            validate289.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-effective.v3") {
              validate289.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-effective.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs3 = errors;
              if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.workRevision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.workRevision, { instancePath: instancePath + "/workRevision", parentData: data, parentDataProperty: "workRevision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.taskRevision !== undefined) {
                  const _errs5 = errors;
                  if (!validate39(data.taskRevision, { instancePath: instancePath + "/taskRevision", parentData: data, parentDataProperty: "taskRevision", rootData })) {
                    vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.workContracts !== undefined) {
                    const _errs6 = errors;
                    if (!validate41(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                      vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.taskOverrides !== undefined) {
                      const _errs7 = errors;
                      if (!validate71(data.taskOverrides, { instancePath: instancePath + "/taskOverrides", parentData: data, parentDataProperty: "taskOverrides", rootData })) {
                        vErrors = vErrors === null ? validate71.errors : vErrors.concat(validate71.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.contracts !== undefined) {
                        const _errs8 = errors;
                        if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                          vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.parentTaskChain !== undefined) {
                          let data7 = data.parentTaskChain;
                          const _errs9 = errors;
                          if (errors === _errs9) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate289.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid1 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs11 = errors;
                                  if (!validate296(data7[i0], { instancePath: instancePath + "/parentTaskChain/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate296.errors : vErrors.concat(validate296.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid1 = _errs11 === errors;
                                  if (!valid1) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate289.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.taskResources !== undefined) {
                            let data9 = data.taskResources;
                            const _errs12 = errors;
                            if (errors === _errs12) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate289.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs14 = errors;
                                    if (!validate77(data9[i1], { instancePath: instancePath + "/taskResources/" + i1, parentData: data9, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs14 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate289.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs12 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.acceptanceCriteria !== undefined) {
                              let data11 = data.acceptanceCriteria;
                              const _errs15 = errors;
                              if (errors === _errs15) {
                                if (Array.isArray(data11)) {
                                  if (data11.length > 2000) {
                                    validate289.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid3 = true;
                                    const len2 = data11.length;
                                    for (let i2 = 0;i2 < len2; i2++) {
                                      const _errs17 = errors;
                                      if (!validate24(data11[i2], { instancePath: instancePath + "/acceptanceCriteria/" + i2, parentData: data11, parentDataProperty: i2, rootData })) {
                                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid3 = _errs17 === errors;
                                      if (!valid3) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate289.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs15 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.dependencies !== undefined) {
                                let data13 = data.dependencies;
                                const _errs18 = errors;
                                if (errors === _errs18) {
                                  if (Array.isArray(data13)) {
                                    if (data13.length > 1e4) {
                                      validate289.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                      return false;
                                    } else {
                                      var valid4 = true;
                                      const len3 = data13.length;
                                      for (let i3 = 0;i3 < len3; i3++) {
                                        const _errs20 = errors;
                                        if (!validate126(data13[i3], { instancePath: instancePath + "/dependencies/" + i3, parentData: data13, parentDataProperty: i3, rootData })) {
                                          vErrors = vErrors === null ? validate126.errors : vErrors.concat(validate126.errors);
                                          errors = vErrors.length;
                                        }
                                        var valid4 = _errs20 === errors;
                                        if (!valid4) {
                                          break;
                                        }
                                      }
                                    }
                                  } else {
                                    validate289.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs18 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.dependencyTasks !== undefined) {
                                  let data15 = data.dependencyTasks;
                                  const _errs21 = errors;
                                  if (errors === _errs21) {
                                    if (Array.isArray(data15)) {
                                      if (data15.length > 2000) {
                                        validate289.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                        return false;
                                      } else {
                                        var valid5 = true;
                                        const len4 = data15.length;
                                        for (let i4 = 0;i4 < len4; i4++) {
                                          const _errs23 = errors;
                                          if (!validate304(data15[i4], { instancePath: instancePath + "/dependencyTasks/" + i4, parentData: data15, parentDataProperty: i4, rootData })) {
                                            vErrors = vErrors === null ? validate304.errors : vErrors.concat(validate304.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid5 = _errs23 === errors;
                                          if (!valid5) {
                                            break;
                                          }
                                        }
                                      }
                                    } else {
                                      validate289.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs21 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.digest !== undefined) {
                                    const _errs24 = errors;
                                    if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid0 = _errs24 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                  if (valid0) {
                                    if (data.legacyBinding !== undefined) {
                                      const _errs25 = errors;
                                      if (!validate80(data.legacyBinding, { instancePath: instancePath + "/legacyBinding", parentData: data, parentDataProperty: "legacyBinding", rootData })) {
                                        vErrors = vErrors === null ? validate80.errors : vErrors.concat(validate80.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid0 = _errs25 === errors;
                                    } else {
                                      var valid0 = true;
                                    }
                                    if (valid0) {
                                      if (data.holdReasons !== undefined) {
                                        let data19 = data.holdReasons;
                                        const _errs26 = errors;
                                        if (errors === _errs26) {
                                          if (Array.isArray(data19)) {
                                            if (data19.length > 2048) {
                                              validate289.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                              return false;
                                            } else {
                                              var valid6 = true;
                                              const len5 = data19.length;
                                              for (let i5 = 0;i5 < len5; i5++) {
                                                const _errs28 = errors;
                                                if (!validate24(data19[i5], { instancePath: instancePath + "/holdReasons/" + i5, parentData: data19, parentDataProperty: i5, rootData })) {
                                                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid6 = _errs28 === errors;
                                                if (!valid6) {
                                                  break;
                                                }
                                              }
                                            }
                                          } else {
                                            validate289.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                            return false;
                                          }
                                        }
                                        var valid0 = _errs26 === errors;
                                      } else {
                                        var valid0 = true;
                                      }
                                      if (valid0) {
                                        if (data.externalArtifacts !== undefined) {
                                          let data21 = data.externalArtifacts;
                                          const _errs29 = errors;
                                          if (errors === _errs29) {
                                            if (Array.isArray(data21)) {
                                              if (data21.length > 2048) {
                                                validate289.errors = [{ instancePath: instancePath + "/externalArtifacts", schemaPath: "#/properties/externalArtifacts/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                                return false;
                                              } else {
                                                var valid7 = true;
                                                const len6 = data21.length;
                                                for (let i6 = 0;i6 < len6; i6++) {
                                                  const _errs31 = errors;
                                                  if (!validate180(data21[i6], { instancePath: instancePath + "/externalArtifacts/" + i6, parentData: data21, parentDataProperty: i6, rootData })) {
                                                    vErrors = vErrors === null ? validate180.errors : vErrors.concat(validate180.errors);
                                                    errors = vErrors.length;
                                                  }
                                                  var valid7 = _errs31 === errors;
                                                  if (!valid7) {
                                                    break;
                                                  }
                                                }
                                              }
                                            } else {
                                              validate289.errors = [{ instancePath: instancePath + "/externalArtifacts", schemaPath: "#/properties/externalArtifacts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                              return false;
                                            }
                                          }
                                          var valid0 = _errs29 === errors;
                                        } else {
                                          var valid0 = true;
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate289.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate289.errors = vErrors;
  return errors === 0;
}
var validateAllocationBinding = validate315;
var schema54 = { type: "object", properties: { schema: { const: "common.git-work-allocation.v3" }, ownerId: { $ref: "#/$defs/Id" }, createRequestId: { $ref: "#/$defs/Id" }, requestDigest: { $ref: "#/$defs/Digest" }, kind: { enum: ["work", "task", "run"] }, scope: { $ref: "#/$defs/Scope" }, id: { type: "string", pattern: "^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$" }, recordPath: { type: "string", maxLength: 4096 }, recordSha256: { $ref: "#/$defs/Digest" }, contracts: { type: "array", maxItems: 100, items: { type: "object", properties: { path: { type: "string", maxLength: 4096 }, sha256: { $ref: "#/$defs/Digest" } }, required: ["path", "sha256"], additionalProperties: false } }, authority: { type: "object", properties: { actorId: { $ref: "#/$defs/Id" }, revision: { $ref: "#/$defs/Revision" }, authorityRef: { $ref: "#/$defs/Id" }, gitHookPolicyRef: { $ref: "#/$defs/Id" } }, required: ["actorId", "revision", "authorityRef", "gitHookPolicyRef"], additionalProperties: false }, creation: { type: "object", properties: { recordBlobOid: { type: "string", pattern: "^[a-f0-9]{40}$" }, contractBlobs: { type: "array", maxItems: 100, items: { type: "object", properties: { path: { type: "string", maxLength: 4096 }, blobOid: { type: "string", pattern: "^[a-f0-9]{40}$" }, sha256: { $ref: "#/$defs/Digest" } }, required: ["path", "blobOid", "sha256"], additionalProperties: false } } }, required: ["recordBlobOid", "contractBlobs"], additionalProperties: false } }, required: ["schema", "ownerId", "createRequestId", "requestDigest", "kind", "scope", "id", "recordPath", "recordSha256", "contracts", "authority", "creation"], additionalProperties: false };
var pattern3 = new RegExp("^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$", "u");
function validate315(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.createRequestId === undefined && (missing0 = "createRequestId") || data.requestDigest === undefined && (missing0 = "requestDigest") || data.kind === undefined && (missing0 = "kind") || data.scope === undefined && (missing0 = "scope") || data.id === undefined && (missing0 = "id") || data.recordPath === undefined && (missing0 = "recordPath") || data.recordSha256 === undefined && (missing0 = "recordSha256") || data.contracts === undefined && (missing0 = "contracts") || data.authority === undefined && (missing0 = "authority") || data.creation === undefined && (missing0 = "creation")) {
        validate315.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema54.properties, key0)) {
            validate315.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-allocation.v3") {
              validate315.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-allocation.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.ownerId, { instancePath: instancePath + "/ownerId", parentData: data, parentDataProperty: "ownerId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.createRequestId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.createRequestId, { instancePath: instancePath + "/createRequestId", parentData: data, parentDataProperty: "createRequestId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.requestDigest !== undefined) {
                  const _errs5 = errors;
                  if (!validate52(data.requestDigest, { instancePath: instancePath + "/requestDigest", parentData: data, parentDataProperty: "requestDigest", rootData })) {
                    vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.kind !== undefined) {
                    let data4 = data.kind;
                    const _errs6 = errors;
                    if (!(data4 === "work" || data4 === "task" || data4 === "run")) {
                      validate315.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema54.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.scope !== undefined) {
                      const _errs7 = errors;
                      if (!validate140(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData })) {
                        vErrors = vErrors === null ? validate140.errors : vErrors.concat(validate140.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.id !== undefined) {
                        let data6 = data.id;
                        const _errs8 = errors;
                        if (errors === _errs8) {
                          if (typeof data6 === "string") {
                            if (!pattern3.test(data6)) {
                              validate315.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/properties/id/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$" }, message: 'must match pattern "' + "^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$" + '"' }];
                              return false;
                            }
                          } else {
                            validate315.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/properties/id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.recordPath !== undefined) {
                          let data7 = data.recordPath;
                          const _errs10 = errors;
                          if (errors === _errs10) {
                            if (typeof data7 === "string") {
                              if (func3(data7) > 4096) {
                                validate315.errors = [{ instancePath: instancePath + "/recordPath", schemaPath: "#/properties/recordPath/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                return false;
                              }
                            } else {
                              validate315.errors = [{ instancePath: instancePath + "/recordPath", schemaPath: "#/properties/recordPath/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                              return false;
                            }
                          }
                          var valid0 = _errs10 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.recordSha256 !== undefined) {
                            const _errs12 = errors;
                            if (!validate52(data.recordSha256, { instancePath: instancePath + "/recordSha256", parentData: data, parentDataProperty: "recordSha256", rootData })) {
                              vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                              errors = vErrors.length;
                            }
                            var valid0 = _errs12 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.contracts !== undefined) {
                              let data9 = data.contracts;
                              const _errs13 = errors;
                              if (errors === _errs13) {
                                if (Array.isArray(data9)) {
                                  if (data9.length > 100) {
                                    validate315.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                    return false;
                                  } else {
                                    var valid1 = true;
                                    const len0 = data9.length;
                                    for (let i0 = 0;i0 < len0; i0++) {
                                      let data10 = data9[i0];
                                      const _errs15 = errors;
                                      if (errors === _errs15) {
                                        if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                                          let missing1;
                                          if (data10.path === undefined && (missing1 = "path") || data10.sha256 === undefined && (missing1 = "sha256")) {
                                            validate315.errors = [{ instancePath: instancePath + "/contracts/" + i0, schemaPath: "#/properties/contracts/items/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                                            return false;
                                          } else {
                                            const _errs17 = errors;
                                            for (const key1 in data10) {
                                              if (!(key1 === "path" || key1 === "sha256")) {
                                                validate315.errors = [{ instancePath: instancePath + "/contracts/" + i0, schemaPath: "#/properties/contracts/items/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                                return false;
                                                break;
                                              }
                                            }
                                            if (_errs17 === errors) {
                                              if (data10.path !== undefined) {
                                                let data11 = data10.path;
                                                const _errs18 = errors;
                                                if (errors === _errs18) {
                                                  if (typeof data11 === "string") {
                                                    if (func3(data11) > 4096) {
                                                      validate315.errors = [{ instancePath: instancePath + "/contracts/" + i0 + "/path", schemaPath: "#/properties/contracts/items/properties/path/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                                      return false;
                                                    }
                                                  } else {
                                                    validate315.errors = [{ instancePath: instancePath + "/contracts/" + i0 + "/path", schemaPath: "#/properties/contracts/items/properties/path/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                    return false;
                                                  }
                                                }
                                                var valid2 = _errs18 === errors;
                                              } else {
                                                var valid2 = true;
                                              }
                                              if (valid2) {
                                                if (data10.sha256 !== undefined) {
                                                  const _errs20 = errors;
                                                  if (!validate52(data10.sha256, { instancePath: instancePath + "/contracts/" + i0 + "/sha256", parentData: data10, parentDataProperty: "sha256", rootData })) {
                                                    vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                                                    errors = vErrors.length;
                                                  }
                                                  var valid2 = _errs20 === errors;
                                                } else {
                                                  var valid2 = true;
                                                }
                                              }
                                            }
                                          }
                                        } else {
                                          validate315.errors = [{ instancePath: instancePath + "/contracts/" + i0, schemaPath: "#/properties/contracts/items/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                          return false;
                                        }
                                      }
                                      var valid1 = _errs15 === errors;
                                      if (!valid1) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate315.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs13 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.authority !== undefined) {
                                let data13 = data.authority;
                                const _errs21 = errors;
                                if (errors === _errs21) {
                                  if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                                    let missing2;
                                    if (data13.actorId === undefined && (missing2 = "actorId") || data13.revision === undefined && (missing2 = "revision") || data13.authorityRef === undefined && (missing2 = "authorityRef") || data13.gitHookPolicyRef === undefined && (missing2 = "gitHookPolicyRef")) {
                                      validate315.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                                      return false;
                                    } else {
                                      const _errs23 = errors;
                                      for (const key2 in data13) {
                                        if (!(key2 === "actorId" || key2 === "revision" || key2 === "authorityRef" || key2 === "gitHookPolicyRef")) {
                                          validate315.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                          return false;
                                          break;
                                        }
                                      }
                                      if (_errs23 === errors) {
                                        if (data13.actorId !== undefined) {
                                          const _errs24 = errors;
                                          if (!validate24(data13.actorId, { instancePath: instancePath + "/authority/actorId", parentData: data13, parentDataProperty: "actorId", rootData })) {
                                            vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid3 = _errs24 === errors;
                                        } else {
                                          var valid3 = true;
                                        }
                                        if (valid3) {
                                          if (data13.revision !== undefined) {
                                            const _errs25 = errors;
                                            if (!validate39(data13.revision, { instancePath: instancePath + "/authority/revision", parentData: data13, parentDataProperty: "revision", rootData })) {
                                              vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                                              errors = vErrors.length;
                                            }
                                            var valid3 = _errs25 === errors;
                                          } else {
                                            var valid3 = true;
                                          }
                                          if (valid3) {
                                            if (data13.authorityRef !== undefined) {
                                              const _errs26 = errors;
                                              if (!validate24(data13.authorityRef, { instancePath: instancePath + "/authority/authorityRef", parentData: data13, parentDataProperty: "authorityRef", rootData })) {
                                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                errors = vErrors.length;
                                              }
                                              var valid3 = _errs26 === errors;
                                            } else {
                                              var valid3 = true;
                                            }
                                            if (valid3) {
                                              if (data13.gitHookPolicyRef !== undefined) {
                                                const _errs27 = errors;
                                                if (!validate24(data13.gitHookPolicyRef, { instancePath: instancePath + "/authority/gitHookPolicyRef", parentData: data13, parentDataProperty: "gitHookPolicyRef", rootData })) {
                                                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid3 = _errs27 === errors;
                                              } else {
                                                var valid3 = true;
                                              }
                                            }
                                          }
                                        }
                                      }
                                    }
                                  } else {
                                    validate315.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs21 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.creation !== undefined) {
                                  let data18 = data.creation;
                                  const _errs28 = errors;
                                  if (errors === _errs28) {
                                    if (data18 && typeof data18 == "object" && !Array.isArray(data18)) {
                                      let missing3;
                                      if (data18.recordBlobOid === undefined && (missing3 = "recordBlobOid") || data18.contractBlobs === undefined && (missing3 = "contractBlobs")) {
                                        validate315.errors = [{ instancePath: instancePath + "/creation", schemaPath: "#/properties/creation/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" }];
                                        return false;
                                      } else {
                                        const _errs30 = errors;
                                        for (const key3 in data18) {
                                          if (!(key3 === "recordBlobOid" || key3 === "contractBlobs")) {
                                            validate315.errors = [{ instancePath: instancePath + "/creation", schemaPath: "#/properties/creation/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key3 }, message: "must NOT have additional properties" }];
                                            return false;
                                            break;
                                          }
                                        }
                                        if (_errs30 === errors) {
                                          if (data18.recordBlobOid !== undefined) {
                                            let data19 = data18.recordBlobOid;
                                            const _errs31 = errors;
                                            if (errors === _errs31) {
                                              if (typeof data19 === "string") {
                                                if (!pattern2.test(data19)) {
                                                  validate315.errors = [{ instancePath: instancePath + "/creation/recordBlobOid", schemaPath: "#/properties/creation/properties/recordBlobOid/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{40}$" }, message: 'must match pattern "' + "^[a-f0-9]{40}$" + '"' }];
                                                  return false;
                                                }
                                              } else {
                                                validate315.errors = [{ instancePath: instancePath + "/creation/recordBlobOid", schemaPath: "#/properties/creation/properties/recordBlobOid/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                return false;
                                              }
                                            }
                                            var valid4 = _errs31 === errors;
                                          } else {
                                            var valid4 = true;
                                          }
                                          if (valid4) {
                                            if (data18.contractBlobs !== undefined) {
                                              let data20 = data18.contractBlobs;
                                              const _errs33 = errors;
                                              if (errors === _errs33) {
                                                if (Array.isArray(data20)) {
                                                  if (data20.length > 100) {
                                                    validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs", schemaPath: "#/properties/creation/properties/contractBlobs/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                                    return false;
                                                  } else {
                                                    var valid5 = true;
                                                    const len1 = data20.length;
                                                    for (let i1 = 0;i1 < len1; i1++) {
                                                      let data21 = data20[i1];
                                                      const _errs35 = errors;
                                                      if (errors === _errs35) {
                                                        if (data21 && typeof data21 == "object" && !Array.isArray(data21)) {
                                                          let missing4;
                                                          if (data21.path === undefined && (missing4 = "path") || data21.blobOid === undefined && (missing4 = "blobOid") || data21.sha256 === undefined && (missing4 = "sha256")) {
                                                            validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs/" + i1, schemaPath: "#/properties/creation/properties/contractBlobs/items/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" }];
                                                            return false;
                                                          } else {
                                                            const _errs37 = errors;
                                                            for (const key4 in data21) {
                                                              if (!(key4 === "path" || key4 === "blobOid" || key4 === "sha256")) {
                                                                validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs/" + i1, schemaPath: "#/properties/creation/properties/contractBlobs/items/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key4 }, message: "must NOT have additional properties" }];
                                                                return false;
                                                                break;
                                                              }
                                                            }
                                                            if (_errs37 === errors) {
                                                              if (data21.path !== undefined) {
                                                                let data22 = data21.path;
                                                                const _errs38 = errors;
                                                                if (errors === _errs38) {
                                                                  if (typeof data22 === "string") {
                                                                    if (func3(data22) > 4096) {
                                                                      validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs/" + i1 + "/path", schemaPath: "#/properties/creation/properties/contractBlobs/items/properties/path/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                                                      return false;
                                                                    }
                                                                  } else {
                                                                    validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs/" + i1 + "/path", schemaPath: "#/properties/creation/properties/contractBlobs/items/properties/path/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                                    return false;
                                                                  }
                                                                }
                                                                var valid6 = _errs38 === errors;
                                                              } else {
                                                                var valid6 = true;
                                                              }
                                                              if (valid6) {
                                                                if (data21.blobOid !== undefined) {
                                                                  let data23 = data21.blobOid;
                                                                  const _errs40 = errors;
                                                                  if (errors === _errs40) {
                                                                    if (typeof data23 === "string") {
                                                                      if (!pattern2.test(data23)) {
                                                                        validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs/" + i1 + "/blobOid", schemaPath: "#/properties/creation/properties/contractBlobs/items/properties/blobOid/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{40}$" }, message: 'must match pattern "' + "^[a-f0-9]{40}$" + '"' }];
                                                                        return false;
                                                                      }
                                                                    } else {
                                                                      validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs/" + i1 + "/blobOid", schemaPath: "#/properties/creation/properties/contractBlobs/items/properties/blobOid/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                                      return false;
                                                                    }
                                                                  }
                                                                  var valid6 = _errs40 === errors;
                                                                } else {
                                                                  var valid6 = true;
                                                                }
                                                                if (valid6) {
                                                                  if (data21.sha256 !== undefined) {
                                                                    const _errs42 = errors;
                                                                    if (!validate52(data21.sha256, { instancePath: instancePath + "/creation/contractBlobs/" + i1 + "/sha256", parentData: data21, parentDataProperty: "sha256", rootData })) {
                                                                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                                                                      errors = vErrors.length;
                                                                    }
                                                                    var valid6 = _errs42 === errors;
                                                                  } else {
                                                                    var valid6 = true;
                                                                  }
                                                                }
                                                              }
                                                            }
                                                          }
                                                        } else {
                                                          validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs/" + i1, schemaPath: "#/properties/creation/properties/contractBlobs/items/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                                          return false;
                                                        }
                                                      }
                                                      var valid5 = _errs35 === errors;
                                                      if (!valid5) {
                                                        break;
                                                      }
                                                    }
                                                  }
                                                } else {
                                                  validate315.errors = [{ instancePath: instancePath + "/creation/contractBlobs", schemaPath: "#/properties/creation/properties/contractBlobs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                                  return false;
                                                }
                                              }
                                              var valid4 = _errs33 === errors;
                                            } else {
                                              var valid4 = true;
                                            }
                                          }
                                        }
                                      }
                                    } else {
                                      validate315.errors = [{ instancePath: instancePath + "/creation", schemaPath: "#/properties/creation/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs28 === errors;
                                } else {
                                  var valid0 = true;
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate315.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate315.errors = vErrors;
  return errors === 0;
}
var validateStoredRecord = validate327;
function validate330(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.runId === undefined && (missing0 = "runId") || data.task === undefined && (missing0 = "task") || data.attempt === undefined && (missing0 = "attempt") || data.status === undefined && (missing0 = "status") || data.effectiveContractDigest === undefined && (missing0 = "effectiveContractDigest") || data.sourceSnapshotId === undefined && (missing0 = "sourceSnapshotId") || data.parentAuthorityRef === undefined && (missing0 = "parentAuthorityRef") || data.nativeBindings === undefined && (missing0 = "nativeBindings") || data.effectRefs === undefined && (missing0 = "effectRefs")) {
        validate330.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema48.properties, key0)) {
            validate330.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.runId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.runId, { instancePath: instancePath + "/runId", parentData: data, parentDataProperty: "runId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs3 = errors;
              if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.attempt !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.attempt, { instancePath: instancePath + "/attempt", parentData: data, parentDataProperty: "attempt", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.status !== undefined) {
                  let data3 = data.status;
                  const _errs5 = errors;
                  if (!(data3 === "reserved" || data3 === "started" || data3 === "succeeded" || data3 === "failed" || data3 === "cancelled" || data3 === "uncertain")) {
                    validate330.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema48.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.effectiveContractDigest !== undefined) {
                    const _errs6 = errors;
                    if (!validate52(data.effectiveContractDigest, { instancePath: instancePath + "/effectiveContractDigest", parentData: data, parentDataProperty: "effectiveContractDigest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.sourceSnapshotId !== undefined) {
                      const _errs7 = errors;
                      if (!validate24(data.sourceSnapshotId, { instancePath: instancePath + "/sourceSnapshotId", parentData: data, parentDataProperty: "sourceSnapshotId", rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.parentAuthorityRef !== undefined) {
                        const _errs8 = errors;
                        if (!validate24(data.parentAuthorityRef, { instancePath: instancePath + "/parentAuthorityRef", parentData: data, parentDataProperty: "parentAuthorityRef", rootData })) {
                          vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.nativeBindings !== undefined) {
                          let data7 = data.nativeBindings;
                          const _errs9 = errors;
                          if (errors === _errs9) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate330.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid1 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs11 = errors;
                                  if (!validate163(data7[i0], { instancePath: instancePath + "/nativeBindings/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate163.errors : vErrors.concat(validate163.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid1 = _errs11 === errors;
                                  if (!valid1) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate330.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.effectRefs !== undefined) {
                            let data9 = data.effectRefs;
                            const _errs12 = errors;
                            if (errors === _errs12) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate330.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs14 = errors;
                                    if (!validate24(data9[i1], { instancePath: instancePath + "/effectRefs/" + i1, parentData: data9, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs14 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate330.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs12 === errors;
                          } else {
                            var valid0 = true;
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate330.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate330.errors = vErrors;
  return errors === 0;
}
function validate327(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (errors === _errs1) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.kind === undefined && (missing0 = "kind") || data.value === undefined && (missing0 = "value")) {
        const err0 = { instancePath, schemaPath: "#/oneOf/0/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        const _errs3 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "kind" || key0 === "value")) {
            const err1 = { instancePath, schemaPath: "#/oneOf/0/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
            break;
          }
        }
        if (_errs3 === errors) {
          if (data.schema !== undefined) {
            const _errs4 = errors;
            if (data.schema !== "common.git-work-record.v3") {
              const err2 = { instancePath: instancePath + "/schema", schemaPath: "#/oneOf/0/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-record.v3" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err2];
              } else {
                vErrors.push(err2);
              }
              errors++;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.kind !== undefined) {
              const _errs5 = errors;
              if (data.kind !== "work") {
                const err3 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/0/properties/kind/const", keyword: "const", params: { allowedValue: "work" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err3];
                } else {
                  vErrors.push(err3);
                }
                errors++;
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.value !== undefined) {
                const _errs6 = errors;
                if (!validate34(data.value, { instancePath: instancePath + "/value", parentData: data, parentDataProperty: "value", rootData })) {
                  vErrors = vErrors === null ? validate34.errors : vErrors.concat(validate34.errors);
                  errors = vErrors.length;
                }
                var valid1 = _errs6 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      }
    } else {
      const err4 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err4];
      } else {
        vErrors.push(err4);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs7 = errors;
  if (errors === _errs7) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.schema === undefined && (missing1 = "schema") || data.kind === undefined && (missing1 = "kind") || data.value === undefined && (missing1 = "value")) {
        const err5 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err5];
        } else {
          vErrors.push(err5);
        }
        errors++;
      } else {
        const _errs9 = errors;
        for (const key1 in data) {
          if (!(key1 === "schema" || key1 === "kind" || key1 === "value")) {
            const err6 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err6];
            } else {
              vErrors.push(err6);
            }
            errors++;
            break;
          }
        }
        if (_errs9 === errors) {
          if (data.schema !== undefined) {
            const _errs10 = errors;
            if (data.schema !== "common.git-work-record.v3") {
              const err7 = { instancePath: instancePath + "/schema", schemaPath: "#/oneOf/1/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-record.v3" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err7];
              } else {
                vErrors.push(err7);
              }
              errors++;
            }
            var valid2 = _errs10 === errors;
          } else {
            var valid2 = true;
          }
          if (valid2) {
            if (data.kind !== undefined) {
              const _errs11 = errors;
              if (data.kind !== "task") {
                const err8 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "task" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err8];
                } else {
                  vErrors.push(err8);
                }
                errors++;
              }
              var valid2 = _errs11 === errors;
            } else {
              var valid2 = true;
            }
            if (valid2) {
              if (data.value !== undefined) {
                const _errs12 = errors;
                if (!validate63(data.value, { instancePath: instancePath + "/value", parentData: data, parentDataProperty: "value", rootData })) {
                  vErrors = vErrors === null ? validate63.errors : vErrors.concat(validate63.errors);
                  errors = vErrors.length;
                }
                var valid2 = _errs12 === errors;
              } else {
                var valid2 = true;
              }
            }
          }
        }
      }
    } else {
      const err9 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err9];
      } else {
        vErrors.push(err9);
      }
      errors++;
    }
  }
  var _valid0 = _errs7 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
    const _errs13 = errors;
    if (errors === _errs13) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.schema === undefined && (missing2 = "schema") || data.kind === undefined && (missing2 = "kind") || data.value === undefined && (missing2 = "value")) {
          const err10 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err10];
          } else {
            vErrors.push(err10);
          }
          errors++;
        } else {
          const _errs15 = errors;
          for (const key2 in data) {
            if (!(key2 === "schema" || key2 === "kind" || key2 === "value")) {
              const err11 = { instancePath, schemaPath: "#/oneOf/2/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" };
              if (vErrors === null) {
                vErrors = [err11];
              } else {
                vErrors.push(err11);
              }
              errors++;
              break;
            }
          }
          if (_errs15 === errors) {
            if (data.schema !== undefined) {
              const _errs16 = errors;
              if (data.schema !== "common.git-work-record.v3") {
                const err12 = { instancePath: instancePath + "/schema", schemaPath: "#/oneOf/2/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-record.v3" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err12];
                } else {
                  vErrors.push(err12);
                }
                errors++;
              }
              var valid3 = _errs16 === errors;
            } else {
              var valid3 = true;
            }
            if (valid3) {
              if (data.kind !== undefined) {
                const _errs17 = errors;
                if (data.kind !== "run") {
                  const err13 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/2/properties/kind/const", keyword: "const", params: { allowedValue: "run" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err13];
                  } else {
                    vErrors.push(err13);
                  }
                  errors++;
                }
                var valid3 = _errs17 === errors;
              } else {
                var valid3 = true;
              }
              if (valid3) {
                if (data.value !== undefined) {
                  const _errs18 = errors;
                  if (!validate330(data.value, { instancePath: instancePath + "/value", parentData: data, parentDataProperty: "value", rootData })) {
                    vErrors = vErrors === null ? validate330.errors : vErrors.concat(validate330.errors);
                    errors = vErrors.length;
                  }
                  var valid3 = _errs18 === errors;
                } else {
                  var valid3 = true;
                }
              }
            }
          }
        }
      } else {
        const err14 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err14];
        } else {
          vErrors.push(err14);
        }
        errors++;
      }
    }
    var _valid0 = _errs13 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
      }
    }
  }
  if (!valid0) {
    const err15 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err15];
    } else {
      vErrors.push(err15);
    }
    errors++;
    validate327.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate327.errors = vErrors;
  return errors === 0;
}
var validateContextPacket = validate340;
var schema57 = { type: "object", properties: { schema: { const: "common.git-work-context.v3" }, mode: { enum: ["full", "compact", "dispatch", "resume"] }, effective: { $ref: "#/$defs/EffectiveContract" }, contractSources: { type: "array", maxItems: 1e4, items: { $ref: "#/$defs/Contract" } }, authority: { const: "none" }, nativePermissionGranted: { const: false }, semanticAcceptanceEstablished: { const: false }, status: { enum: ["source-qualified", "held"] }, holdReasons: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2048 } }, required: ["schema", "mode", "effective", "contractSources", "authority", "nativePermissionGranted", "semanticAcceptanceEstablished", "status", "holdReasons"], additionalProperties: false };
function validate341(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.taskOverrides === undefined && (missing0 = "taskOverrides") || data.contracts === undefined && (missing0 = "contracts") || data.parentTaskChain === undefined && (missing0 = "parentTaskChain") || data.taskResources === undefined && (missing0 = "taskResources") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.dependencies === undefined && (missing0 = "dependencies") || data.dependencyTasks === undefined && (missing0 = "dependencyTasks") || data.digest === undefined && (missing0 = "digest") || data.holdReasons === undefined && (missing0 = "holdReasons") || data.externalArtifacts === undefined && (missing0 = "externalArtifacts")) {
        validate341.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema51.properties, key0)) {
            validate341.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-effective.v3") {
              validate341.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-effective.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs3 = errors;
              if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.workRevision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.workRevision, { instancePath: instancePath + "/workRevision", parentData: data, parentDataProperty: "workRevision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.taskRevision !== undefined) {
                  const _errs5 = errors;
                  if (!validate39(data.taskRevision, { instancePath: instancePath + "/taskRevision", parentData: data, parentDataProperty: "taskRevision", rootData })) {
                    vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.workContracts !== undefined) {
                    const _errs6 = errors;
                    if (!validate41(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                      vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.taskOverrides !== undefined) {
                      const _errs7 = errors;
                      if (!validate71(data.taskOverrides, { instancePath: instancePath + "/taskOverrides", parentData: data, parentDataProperty: "taskOverrides", rootData })) {
                        vErrors = vErrors === null ? validate71.errors : vErrors.concat(validate71.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.contracts !== undefined) {
                        const _errs8 = errors;
                        if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                          vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.parentTaskChain !== undefined) {
                          let data7 = data.parentTaskChain;
                          const _errs9 = errors;
                          if (errors === _errs9) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate341.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid1 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs11 = errors;
                                  if (!validate296(data7[i0], { instancePath: instancePath + "/parentTaskChain/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate296.errors : vErrors.concat(validate296.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid1 = _errs11 === errors;
                                  if (!valid1) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate341.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.taskResources !== undefined) {
                            let data9 = data.taskResources;
                            const _errs12 = errors;
                            if (errors === _errs12) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate341.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs14 = errors;
                                    if (!validate77(data9[i1], { instancePath: instancePath + "/taskResources/" + i1, parentData: data9, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs14 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate341.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs12 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.acceptanceCriteria !== undefined) {
                              let data11 = data.acceptanceCriteria;
                              const _errs15 = errors;
                              if (errors === _errs15) {
                                if (Array.isArray(data11)) {
                                  if (data11.length > 2000) {
                                    validate341.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid3 = true;
                                    const len2 = data11.length;
                                    for (let i2 = 0;i2 < len2; i2++) {
                                      const _errs17 = errors;
                                      if (!validate24(data11[i2], { instancePath: instancePath + "/acceptanceCriteria/" + i2, parentData: data11, parentDataProperty: i2, rootData })) {
                                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid3 = _errs17 === errors;
                                      if (!valid3) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate341.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs15 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.dependencies !== undefined) {
                                let data13 = data.dependencies;
                                const _errs18 = errors;
                                if (errors === _errs18) {
                                  if (Array.isArray(data13)) {
                                    if (data13.length > 1e4) {
                                      validate341.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                      return false;
                                    } else {
                                      var valid4 = true;
                                      const len3 = data13.length;
                                      for (let i3 = 0;i3 < len3; i3++) {
                                        const _errs20 = errors;
                                        if (!validate126(data13[i3], { instancePath: instancePath + "/dependencies/" + i3, parentData: data13, parentDataProperty: i3, rootData })) {
                                          vErrors = vErrors === null ? validate126.errors : vErrors.concat(validate126.errors);
                                          errors = vErrors.length;
                                        }
                                        var valid4 = _errs20 === errors;
                                        if (!valid4) {
                                          break;
                                        }
                                      }
                                    }
                                  } else {
                                    validate341.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs18 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.dependencyTasks !== undefined) {
                                  let data15 = data.dependencyTasks;
                                  const _errs21 = errors;
                                  if (errors === _errs21) {
                                    if (Array.isArray(data15)) {
                                      if (data15.length > 2000) {
                                        validate341.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                        return false;
                                      } else {
                                        var valid5 = true;
                                        const len4 = data15.length;
                                        for (let i4 = 0;i4 < len4; i4++) {
                                          const _errs23 = errors;
                                          if (!validate304(data15[i4], { instancePath: instancePath + "/dependencyTasks/" + i4, parentData: data15, parentDataProperty: i4, rootData })) {
                                            vErrors = vErrors === null ? validate304.errors : vErrors.concat(validate304.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid5 = _errs23 === errors;
                                          if (!valid5) {
                                            break;
                                          }
                                        }
                                      }
                                    } else {
                                      validate341.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs21 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.digest !== undefined) {
                                    const _errs24 = errors;
                                    if (!validate52(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid0 = _errs24 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                  if (valid0) {
                                    if (data.legacyBinding !== undefined) {
                                      const _errs25 = errors;
                                      if (!validate80(data.legacyBinding, { instancePath: instancePath + "/legacyBinding", parentData: data, parentDataProperty: "legacyBinding", rootData })) {
                                        vErrors = vErrors === null ? validate80.errors : vErrors.concat(validate80.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid0 = _errs25 === errors;
                                    } else {
                                      var valid0 = true;
                                    }
                                    if (valid0) {
                                      if (data.holdReasons !== undefined) {
                                        let data19 = data.holdReasons;
                                        const _errs26 = errors;
                                        if (errors === _errs26) {
                                          if (Array.isArray(data19)) {
                                            if (data19.length > 2048) {
                                              validate341.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                              return false;
                                            } else {
                                              var valid6 = true;
                                              const len5 = data19.length;
                                              for (let i5 = 0;i5 < len5; i5++) {
                                                const _errs28 = errors;
                                                if (!validate24(data19[i5], { instancePath: instancePath + "/holdReasons/" + i5, parentData: data19, parentDataProperty: i5, rootData })) {
                                                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid6 = _errs28 === errors;
                                                if (!valid6) {
                                                  break;
                                                }
                                              }
                                            }
                                          } else {
                                            validate341.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                            return false;
                                          }
                                        }
                                        var valid0 = _errs26 === errors;
                                      } else {
                                        var valid0 = true;
                                      }
                                      if (valid0) {
                                        if (data.externalArtifacts !== undefined) {
                                          let data21 = data.externalArtifacts;
                                          const _errs29 = errors;
                                          if (errors === _errs29) {
                                            if (Array.isArray(data21)) {
                                              if (data21.length > 2048) {
                                                validate341.errors = [{ instancePath: instancePath + "/externalArtifacts", schemaPath: "#/properties/externalArtifacts/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                                return false;
                                              } else {
                                                var valid7 = true;
                                                const len6 = data21.length;
                                                for (let i6 = 0;i6 < len6; i6++) {
                                                  const _errs31 = errors;
                                                  if (!validate180(data21[i6], { instancePath: instancePath + "/externalArtifacts/" + i6, parentData: data21, parentDataProperty: i6, rootData })) {
                                                    vErrors = vErrors === null ? validate180.errors : vErrors.concat(validate180.errors);
                                                    errors = vErrors.length;
                                                  }
                                                  var valid7 = _errs31 === errors;
                                                  if (!valid7) {
                                                    break;
                                                  }
                                                }
                                              }
                                            } else {
                                              validate341.errors = [{ instancePath: instancePath + "/externalArtifacts", schemaPath: "#/properties/externalArtifacts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                              return false;
                                            }
                                          }
                                          var valid0 = _errs29 === errors;
                                        } else {
                                          var valid0 = true;
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate341.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate341.errors = vErrors;
  return errors === 0;
}
function validate340(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.mode === undefined && (missing0 = "mode") || data.effective === undefined && (missing0 = "effective") || data.contractSources === undefined && (missing0 = "contractSources") || data.authority === undefined && (missing0 = "authority") || data.nativePermissionGranted === undefined && (missing0 = "nativePermissionGranted") || data.semanticAcceptanceEstablished === undefined && (missing0 = "semanticAcceptanceEstablished") || data.status === undefined && (missing0 = "status") || data.holdReasons === undefined && (missing0 = "holdReasons")) {
        validate340.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema57.properties, key0)) {
            validate340.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-context.v3") {
              validate340.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-context.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.mode !== undefined) {
              let data1 = data.mode;
              const _errs3 = errors;
              if (!(data1 === "full" || data1 === "compact" || data1 === "dispatch" || data1 === "resume")) {
                validate340.errors = [{ instancePath: instancePath + "/mode", schemaPath: "#/properties/mode/enum", keyword: "enum", params: { allowedValues: schema57.properties.mode.enum }, message: "must be equal to one of the allowed values" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.effective !== undefined) {
                const _errs4 = errors;
                if (!validate341(data.effective, { instancePath: instancePath + "/effective", parentData: data, parentDataProperty: "effective", rootData })) {
                  vErrors = vErrors === null ? validate341.errors : vErrors.concat(validate341.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.contractSources !== undefined) {
                  let data3 = data.contractSources;
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 1e4) {
                        validate340.errors = [{ instancePath: instancePath + "/contractSources", schemaPath: "#/properties/contractSources/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate86(data3[i0], { instancePath: instancePath + "/contractSources/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate86.errors : vErrors.concat(validate86.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate340.errors = [{ instancePath: instancePath + "/contractSources", schemaPath: "#/properties/contractSources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.authority !== undefined) {
                    const _errs8 = errors;
                    if (data.authority !== "none") {
                      validate340.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/const", keyword: "const", params: { allowedValue: "none" }, message: "must be equal to constant" }];
                      return false;
                    }
                    var valid0 = _errs8 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.nativePermissionGranted !== undefined) {
                      const _errs9 = errors;
                      if (data.nativePermissionGranted !== false) {
                        validate340.errors = [{ instancePath: instancePath + "/nativePermissionGranted", schemaPath: "#/properties/nativePermissionGranted/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
                        return false;
                      }
                      var valid0 = _errs9 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.semanticAcceptanceEstablished !== undefined) {
                        const _errs10 = errors;
                        if (data.semanticAcceptanceEstablished !== false) {
                          validate340.errors = [{ instancePath: instancePath + "/semanticAcceptanceEstablished", schemaPath: "#/properties/semanticAcceptanceEstablished/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
                          return false;
                        }
                        var valid0 = _errs10 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.status !== undefined) {
                          let data8 = data.status;
                          const _errs11 = errors;
                          if (!(data8 === "source-qualified" || data8 === "held")) {
                            validate340.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema57.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                            return false;
                          }
                          var valid0 = _errs11 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.holdReasons !== undefined) {
                            let data9 = data.holdReasons;
                            const _errs12 = errors;
                            if (errors === _errs12) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2048) {
                                  validate340.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs14 = errors;
                                    if (!validate24(data9[i1], { instancePath: instancePath + "/holdReasons/" + i1, parentData: data9, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs14 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate340.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs12 === errors;
                          } else {
                            var valid0 = true;
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate340.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate340.errors = vErrors;
  return errors === 0;
}
var validateCompactContextPacket = validate360;
function validate360(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.encoding === undefined && (missing0 = "encoding") || data.effectiveDigest === undefined && (missing0 = "effectiveDigest") || data.decodedBytes === undefined && (missing0 = "decodedBytes") || data.payload === undefined && (missing0 = "payload") || data.authority === undefined && (missing0 = "authority") || data.nativePermissionGranted === undefined && (missing0 = "nativePermissionGranted") || data.semanticAcceptanceEstablished === undefined && (missing0 = "semanticAcceptanceEstablished")) {
        validate360.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "encoding" || key0 === "effectiveDigest" || key0 === "decodedBytes" || key0 === "payload" || key0 === "authority" || key0 === "nativePermissionGranted" || key0 === "semanticAcceptanceEstablished")) {
            validate360.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-context.compact.v3") {
              validate360.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-context.compact.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.encoding !== undefined) {
              const _errs3 = errors;
              if (data.encoding !== "deflate-raw+base64.v1") {
                validate360.errors = [{ instancePath: instancePath + "/encoding", schemaPath: "#/properties/encoding/const", keyword: "const", params: { allowedValue: "deflate-raw+base64.v1" }, message: "must be equal to constant" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.effectiveDigest !== undefined) {
                const _errs4 = errors;
                if (!validate52(data.effectiveDigest, { instancePath: instancePath + "/effectiveDigest", parentData: data, parentDataProperty: "effectiveDigest", rootData })) {
                  vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.decodedBytes !== undefined) {
                  let data3 = data.decodedBytes;
                  const _errs5 = errors;
                  if (!(typeof data3 == "number" && (!(data3 % 1) && !isNaN(data3)) && isFinite(data3))) {
                    validate360.errors = [{ instancePath: instancePath + "/decodedBytes", schemaPath: "#/properties/decodedBytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                    return false;
                  }
                  if (errors === _errs5) {
                    if (typeof data3 == "number" && isFinite(data3)) {
                      if (data3 > 4194304 || isNaN(data3)) {
                        validate360.errors = [{ instancePath: instancePath + "/decodedBytes", schemaPath: "#/properties/decodedBytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 4194304 }, message: "must be <= 4194304" }];
                        return false;
                      } else {
                        if (data3 < 1 || isNaN(data3)) {
                          validate360.errors = [{ instancePath: instancePath + "/decodedBytes", schemaPath: "#/properties/decodedBytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                          return false;
                        }
                      }
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.payload !== undefined) {
                    let data4 = data.payload;
                    const _errs7 = errors;
                    if (errors === _errs7) {
                      if (typeof data4 === "string") {
                        if (func3(data4) > 8388608) {
                          validate360.errors = [{ instancePath: instancePath + "/payload", schemaPath: "#/properties/payload/maxLength", keyword: "maxLength", params: { limit: 8388608 }, message: "must NOT have more than 8388608 characters" }];
                          return false;
                        } else {
                          if (func3(data4) < 1) {
                            validate360.errors = [{ instancePath: instancePath + "/payload", schemaPath: "#/properties/payload/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                            return false;
                          }
                        }
                      } else {
                        validate360.errors = [{ instancePath: instancePath + "/payload", schemaPath: "#/properties/payload/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid0 = _errs7 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.authority !== undefined) {
                      const _errs9 = errors;
                      if (data.authority !== "none") {
                        validate360.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/const", keyword: "const", params: { allowedValue: "none" }, message: "must be equal to constant" }];
                        return false;
                      }
                      var valid0 = _errs9 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.nativePermissionGranted !== undefined) {
                        const _errs10 = errors;
                        if (data.nativePermissionGranted !== false) {
                          validate360.errors = [{ instancePath: instancePath + "/nativePermissionGranted", schemaPath: "#/properties/nativePermissionGranted/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
                          return false;
                        }
                        var valid0 = _errs10 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.semanticAcceptanceEstablished !== undefined) {
                          const _errs11 = errors;
                          if (data.semanticAcceptanceEstablished !== false) {
                            validate360.errors = [{ instancePath: instancePath + "/semanticAcceptanceEstablished", schemaPath: "#/properties/semanticAcceptanceEstablished/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
                            return false;
                          }
                          var valid0 = _errs11 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate360.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate360.errors = vErrors;
  return errors === 0;
}
var validateGitSourcePin = validate362;
function validate362(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.repositoryId === undefined && (missing0 = "repositoryId") || data.commit === undefined && (missing0 = "commit") || data.path === undefined && (missing0 = "path") || data.sha256 === undefined && (missing0 = "sha256")) {
        validate362.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "repositoryId" || key0 === "commit" || key0 === "path" || key0 === "sha256")) {
            validate362.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.repositoryId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.repositoryId, { instancePath: instancePath + "/repositoryId", parentData: data, parentDataProperty: "repositoryId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.commit !== undefined) {
              let data1 = data.commit;
              const _errs3 = errors;
              if (errors === _errs3) {
                if (typeof data1 === "string") {
                  if (!pattern2.test(data1)) {
                    validate362.errors = [{ instancePath: instancePath + "/commit", schemaPath: "#/properties/commit/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{40}$" }, message: 'must match pattern "' + "^[a-f0-9]{40}$" + '"' }];
                    return false;
                  }
                } else {
                  validate362.errors = [{ instancePath: instancePath + "/commit", schemaPath: "#/properties/commit/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.path !== undefined) {
                let data2 = data.path;
                const _errs5 = errors;
                if (errors === _errs5) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 4096) {
                      validate362.errors = [{ instancePath: instancePath + "/path", schemaPath: "#/properties/path/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate362.errors = [{ instancePath: instancePath + "/path", schemaPath: "#/properties/path/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      }
                    }
                  } else {
                    validate362.errors = [{ instancePath: instancePath + "/path", schemaPath: "#/properties/path/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.sha256 !== undefined) {
                  const _errs7 = errors;
                  if (!validate52(data.sha256, { instancePath: instancePath + "/sha256", parentData: data, parentDataProperty: "sha256", rootData })) {
                    vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate362.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate362.errors = vErrors;
  return errors === 0;
}
var validateLegacyTaskBinding = validate365;
function validate365(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.protocol === undefined && (missing0 = "protocol") || data.source === undefined && (missing0 = "source") || data.contracts === undefined && (missing0 = "contracts") || data.holdReasons === undefined && (missing0 = "holdReasons")) {
        validate365.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "protocol" || key0 === "source" || key0 === "contracts" || key0 === "holdReasons")) {
            validate365.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.protocol !== undefined) {
            let data0 = data.protocol;
            const _errs2 = errors;
            if (!(data0 === "common.git-work.v1" || data0 === "common.git-work-task.v2")) {
              validate365.errors = [{ instancePath: instancePath + "/protocol", schemaPath: "#/properties/protocol/enum", keyword: "enum", params: { allowedValues: schema21.properties.protocol.enum }, message: "must be equal to one of the allowed values" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.source !== undefined) {
              const _errs3 = errors;
              if (!validate58(data.source, { instancePath: instancePath + "/source", parentData: data, parentDataProperty: "source", rootData })) {
                vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.contracts !== undefined) {
                const _errs4 = errors;
                if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                  vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.holdReasons !== undefined) {
                  let data3 = data.holdReasons;
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 2048) {
                        validate365.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate24(data3[i0], { instancePath: instancePath + "/holdReasons/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate365.errors = [{ instancePath: instancePath + "/holdReasons", schemaPath: "#/properties/holdReasons/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate365.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate365.errors = vErrors;
  return errors === 0;
}
var validateGitWorkAlias = validate369;
function validate369(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.namespace === undefined && (missing0 = "namespace") || data.from === undefined && (missing0 = "from") || data.to === undefined && (missing0 = "to") || data.revision === undefined && (missing0 = "revision") || data.provenance === undefined && (missing0 = "provenance") || data.reason === undefined && (missing0 = "reason")) {
        validate369.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "namespace" || key0 === "from" || key0 === "to" || key0 === "revision" || key0 === "provenance" || key0 === "reason")) {
            validate369.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.namespace !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.namespace, { instancePath: instancePath + "/namespace", parentData: data, parentDataProperty: "namespace", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.from !== undefined) {
              const _errs3 = errors;
              if (!validate173(data.from, { instancePath: instancePath + "/from", parentData: data, parentDataProperty: "from", rootData })) {
                vErrors = vErrors === null ? validate173.errors : vErrors.concat(validate173.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.to !== undefined) {
                const _errs4 = errors;
                if (!validate173(data.to, { instancePath: instancePath + "/to", parentData: data, parentDataProperty: "to", rootData })) {
                  vErrors = vErrors === null ? validate173.errors : vErrors.concat(validate173.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.revision !== undefined) {
                  const _errs5 = errors;
                  if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                    vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.provenance !== undefined) {
                    const _errs6 = errors;
                    if (!validate58(data.provenance, { instancePath: instancePath + "/provenance", parentData: data, parentDataProperty: "provenance", rootData })) {
                      vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.reason !== undefined) {
                      let data5 = data.reason;
                      const _errs7 = errors;
                      if (errors === _errs7) {
                        if (typeof data5 === "string") {
                          if (func3(data5) > 4096) {
                            validate369.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                            return false;
                          } else {
                            if (func3(data5) < 1) {
                              validate369.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            }
                          }
                        } else {
                          validate369.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate369.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate369.errors = vErrors;
  return errors === 0;
}
var validateConsumerContractBinding = validate375;
function validate375(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.contracts === undefined && (missing0 = "contracts") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria")) {
        validate375.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "workRevision" || key0 === "taskRevision" || key0 === "workContracts" || key0 === "contracts" || key0 === "acceptanceCriteria")) {
            validate375.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workRevision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.workRevision, { instancePath: instancePath + "/workRevision", parentData: data, parentDataProperty: "workRevision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskRevision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.taskRevision, { instancePath: instancePath + "/taskRevision", parentData: data, parentDataProperty: "taskRevision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.workContracts !== undefined) {
                  const _errs5 = errors;
                  if (!validate41(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                    vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.contracts !== undefined) {
                    const _errs6 = errors;
                    if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                      vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.acceptanceCriteria !== undefined) {
                      let data5 = data.acceptanceCriteria;
                      const _errs7 = errors;
                      if (errors === _errs7) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2048) {
                            validate375.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                            return false;
                          } else {
                            var valid1 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs9 = errors;
                              if (!validate24(data5[i0], { instancePath: instancePath + "/acceptanceCriteria/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                errors = vErrors.length;
                              }
                              var valid1 = _errs9 === errors;
                              if (!valid1) {
                                break;
                              }
                            }
                          }
                        } else {
                          validate375.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                          return false;
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate375.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate375.errors = vErrors;
  return errors === 0;
}
var validateProducerContractBinding = validate382;
function validate382(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.contracts === undefined && (missing0 = "contracts") || data.effectiveDigest === undefined && (missing0 = "effectiveDigest")) {
        validate382.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "workRevision" || key0 === "taskRevision" || key0 === "workContracts" || key0 === "contracts" || key0 === "effectiveDigest")) {
            validate382.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate127(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate127.errors : vErrors.concat(validate127.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workRevision !== undefined) {
              const _errs3 = errors;
              if (!validate39(data.workRevision, { instancePath: instancePath + "/workRevision", parentData: data, parentDataProperty: "workRevision", rootData })) {
                vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskRevision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.taskRevision, { instancePath: instancePath + "/taskRevision", parentData: data, parentDataProperty: "taskRevision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.workContracts !== undefined) {
                  const _errs5 = errors;
                  if (!validate41(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                    vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.contracts !== undefined) {
                    const _errs6 = errors;
                    if (!validate41(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                      vErrors = vErrors === null ? validate41.errors : vErrors.concat(validate41.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.effectiveDigest !== undefined) {
                      const _errs7 = errors;
                      if (!validate52(data.effectiveDigest, { instancePath: instancePath + "/effectiveDigest", parentData: data, parentDataProperty: "effectiveDigest", rootData })) {
                        vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate382.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate382.errors = vErrors;
  return errors === 0;
}
var validateSelectedArtifactGate = validate389;
function validate394(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.producer === undefined && (missing0 = "producer") || data.artifact === undefined && (missing0 = "artifact") || data.criterionIds === undefined && (missing0 = "criterionIds")) {
        validate394.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "producer" || key0 === "artifact" || key0 === "criterionIds")) {
            validate394.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.producer !== undefined) {
              const _errs3 = errors;
              if (!validate181(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
                vErrors = vErrors === null ? validate181.errors : vErrors.concat(validate181.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.artifact !== undefined) {
                const _errs4 = errors;
                if (!validate133(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                  vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.criterionIds !== undefined) {
                  let data3 = data.criterionIds;
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 2048) {
                        validate394.errors = [{ instancePath: instancePath + "/criterionIds", schemaPath: "#/properties/criterionIds/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate24(data3[i0], { instancePath: instancePath + "/criterionIds/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate394.errors = [{ instancePath: instancePath + "/criterionIds", schemaPath: "#/properties/criterionIds/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate394.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate394.errors = vErrors;
  return errors === 0;
}
function validate389(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.instanceId === undefined && (missing0 = "instanceId") || data.revision === undefined && (missing0 = "revision") || data.consumer === undefined && (missing0 = "consumer") || data.selectionOwnerId === undefined && (missing0 = "selectionOwnerId") || data.alternatives === undefined && (missing0 = "alternatives")) {
        validate389.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "instanceId" || key0 === "revision" || key0 === "consumer" || key0 === "selectionOwnerId" || key0 === "alternatives")) {
            validate389.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-artifact-gate.v3") {
              validate389.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-artifact-gate.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.instanceId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.instanceId, { instancePath: instancePath + "/instanceId", parentData: data, parentDataProperty: "instanceId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.revision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.consumer !== undefined) {
                  const _errs5 = errors;
                  if (!validate197(data.consumer, { instancePath: instancePath + "/consumer", parentData: data, parentDataProperty: "consumer", rootData })) {
                    vErrors = vErrors === null ? validate197.errors : vErrors.concat(validate197.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.selectionOwnerId !== undefined) {
                    const _errs6 = errors;
                    if (!validate24(data.selectionOwnerId, { instancePath: instancePath + "/selectionOwnerId", parentData: data, parentDataProperty: "selectionOwnerId", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.alternatives !== undefined) {
                      let data5 = data.alternatives;
                      const _errs7 = errors;
                      if (errors === _errs7) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2048) {
                            validate389.errors = [{ instancePath: instancePath + "/alternatives", schemaPath: "#/properties/alternatives/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                            return false;
                          } else {
                            var valid1 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs9 = errors;
                              if (!validate394(data5[i0], { instancePath: instancePath + "/alternatives/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate394.errors : vErrors.concat(validate394.errors);
                                errors = vErrors.length;
                              }
                              var valid1 = _errs9 === errors;
                              if (!valid1) {
                                break;
                              }
                            }
                          }
                        } else {
                          validate389.errors = [{ instancePath: instancePath + "/alternatives", schemaPath: "#/properties/alternatives/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                          return false;
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate389.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate389.errors = vErrors;
  return errors === 0;
}
var validateSelectedArtifactDocument = validate400;
function validate400(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.id === undefined && (missing0 = "id") || data.revision === undefined && (missing0 = "revision") || data.producer === undefined && (missing0 = "producer") || data.body === undefined && (missing0 = "body")) {
        validate400.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "id" || key0 === "revision" || key0 === "producer" || key0 === "body")) {
            validate400.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-artifact.v3") {
              validate400.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-artifact.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.id !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.revision !== undefined) {
                const _errs4 = errors;
                if (!validate39(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.producer !== undefined) {
                  const _errs5 = errors;
                  if (!validate181(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
                    vErrors = vErrors === null ? validate181.errors : vErrors.concat(validate181.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.body !== undefined) {
                    const _errs6 = errors;
                    if (!validate97(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                      vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate400.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate400.errors = vErrors;
  return errors === 0;
}
var validateArtifactSelectionReceipt = validate405;
function validate405(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.selection === undefined && (missing0 = "selection")) {
        validate405.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "selection")) {
            validate405.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-artifact-selection.v3") {
              validate405.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-artifact-selection.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.selection !== undefined) {
              const _errs3 = errors;
              if (!validate193(data.selection, { instancePath: instancePath + "/selection", parentData: data, parentDataProperty: "selection", rootData })) {
                vErrors = vErrors === null ? validate193.errors : vErrors.concat(validate193.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate405.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate405.errors = vErrors;
  return errors === 0;
}
var validateArtifactCriterionReceipt = validate407;
var schema69 = { type: "object", properties: { schema: { const: "common.git-artifact-criterion.v3" }, selection: { $ref: "#/$defs/ArtifactGateSelection" }, criterionId: { $ref: "#/$defs/Id" }, status: { enum: ["pass", "fail", "unknown"] }, evidence: { type: "array", items: { $ref: "#/$defs/GitSourcePin" }, maxItems: 2048 } }, required: ["schema", "selection", "criterionId", "status", "evidence"], additionalProperties: false };
function validate407(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.selection === undefined && (missing0 = "selection") || data.criterionId === undefined && (missing0 = "criterionId") || data.status === undefined && (missing0 = "status") || data.evidence === undefined && (missing0 = "evidence")) {
        validate407.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "selection" || key0 === "criterionId" || key0 === "status" || key0 === "evidence")) {
            validate407.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-artifact-criterion.v3") {
              validate407.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-artifact-criterion.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.selection !== undefined) {
              const _errs3 = errors;
              if (!validate193(data.selection, { instancePath: instancePath + "/selection", parentData: data, parentDataProperty: "selection", rootData })) {
                vErrors = vErrors === null ? validate193.errors : vErrors.concat(validate193.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.criterionId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.criterionId, { instancePath: instancePath + "/criterionId", parentData: data, parentDataProperty: "criterionId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.status !== undefined) {
                  let data3 = data.status;
                  const _errs5 = errors;
                  if (!(data3 === "pass" || data3 === "fail" || data3 === "unknown")) {
                    validate407.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema69.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.evidence !== undefined) {
                    let data4 = data.evidence;
                    const _errs6 = errors;
                    if (errors === _errs6) {
                      if (Array.isArray(data4)) {
                        if (data4.length > 2048) {
                          validate407.errors = [{ instancePath: instancePath + "/evidence", schemaPath: "#/properties/evidence/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                          return false;
                        } else {
                          var valid1 = true;
                          const len0 = data4.length;
                          for (let i0 = 0;i0 < len0; i0++) {
                            const _errs8 = errors;
                            if (!validate58(data4[i0], { instancePath: instancePath + "/evidence/" + i0, parentData: data4, parentDataProperty: i0, rootData })) {
                              vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                              errors = vErrors.length;
                            }
                            var valid1 = _errs8 === errors;
                            if (!valid1) {
                              break;
                            }
                          }
                        }
                      } else {
                        validate407.errors = [{ instancePath: instancePath + "/evidence", schemaPath: "#/properties/evidence/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                        return false;
                      }
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate407.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate407.errors = vErrors;
  return errors === 0;
}
var validateArtifactProducerMilestone = validate411;
var schema70 = { type: "object", properties: { schema: { const: "common.git-artifact-producer-milestone.v3" }, producer: { $ref: "#/$defs/ProducerContractBinding" }, artifact: { $ref: "#/$defs/ArtifactRef" }, artifactSource: { $ref: "#/$defs/GitSourcePin" }, milestone: { enum: ["verified", "accepted"] }, acceptanceDecision: { $ref: "#/$defs/GitSourcePin" } }, required: ["schema", "producer", "artifact", "artifactSource", "milestone"], additionalProperties: false };
function validate411(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.producer === undefined && (missing0 = "producer") || data.artifact === undefined && (missing0 = "artifact") || data.artifactSource === undefined && (missing0 = "artifactSource") || data.milestone === undefined && (missing0 = "milestone")) {
        validate411.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "producer" || key0 === "artifact" || key0 === "artifactSource" || key0 === "milestone" || key0 === "acceptanceDecision")) {
            validate411.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-artifact-producer-milestone.v3") {
              validate411.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-artifact-producer-milestone.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.producer !== undefined) {
              const _errs3 = errors;
              if (!validate181(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
                vErrors = vErrors === null ? validate181.errors : vErrors.concat(validate181.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.artifact !== undefined) {
                const _errs4 = errors;
                if (!validate133(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                  vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.artifactSource !== undefined) {
                  const _errs5 = errors;
                  if (!validate58(data.artifactSource, { instancePath: instancePath + "/artifactSource", parentData: data, parentDataProperty: "artifactSource", rootData })) {
                    vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.milestone !== undefined) {
                    let data4 = data.milestone;
                    const _errs6 = errors;
                    if (!(data4 === "verified" || data4 === "accepted")) {
                      validate411.errors = [{ instancePath: instancePath + "/milestone", schemaPath: "#/properties/milestone/enum", keyword: "enum", params: { allowedValues: schema70.properties.milestone.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.acceptanceDecision !== undefined) {
                      const _errs7 = errors;
                      if (!validate58(data.acceptanceDecision, { instancePath: instancePath + "/acceptanceDecision", parentData: data, parentDataProperty: "acceptanceDecision", rootData })) {
                        vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate411.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate411.errors = vErrors;
  return errors === 0;
}
var validateArtifactAcceptanceDecision = validate416;
function validate416(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.producer === undefined && (missing0 = "producer") || data.artifact === undefined && (missing0 = "artifact") || data.ownerId === undefined && (missing0 = "ownerId") || data.decision === undefined && (missing0 = "decision") || data.evidence === undefined && (missing0 = "evidence")) {
        validate416.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "producer" || key0 === "artifact" || key0 === "ownerId" || key0 === "decision" || key0 === "evidence")) {
            validate416.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-artifact-acceptance.v3") {
              validate416.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-artifact-acceptance.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.producer !== undefined) {
              const _errs3 = errors;
              if (!validate181(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
                vErrors = vErrors === null ? validate181.errors : vErrors.concat(validate181.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.artifact !== undefined) {
                const _errs4 = errors;
                if (!validate133(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                  vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.ownerId !== undefined) {
                  const _errs5 = errors;
                  if (!validate24(data.ownerId, { instancePath: instancePath + "/ownerId", parentData: data, parentDataProperty: "ownerId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.decision !== undefined) {
                    const _errs6 = errors;
                    if (data.decision !== "accepted") {
                      validate416.errors = [{ instancePath: instancePath + "/decision", schemaPath: "#/properties/decision/const", keyword: "const", params: { allowedValue: "accepted" }, message: "must be equal to constant" }];
                      return false;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.evidence !== undefined) {
                      let data5 = data.evidence;
                      const _errs7 = errors;
                      if (errors === _errs7) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2048) {
                            validate416.errors = [{ instancePath: instancePath + "/evidence", schemaPath: "#/properties/evidence/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                            return false;
                          } else {
                            var valid1 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs9 = errors;
                              if (!validate58(data5[i0], { instancePath: instancePath + "/evidence/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                errors = vErrors.length;
                              }
                              var valid1 = _errs9 === errors;
                              if (!valid1) {
                                break;
                              }
                            }
                          }
                        } else {
                          validate416.errors = [{ instancePath: instancePath + "/evidence", schemaPath: "#/properties/evidence/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                          return false;
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate416.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate416.errors = vErrors;
  return errors === 0;
}
var validateArtifactReceiverUse = validate421;
var schema72 = { type: "object", properties: { schema: { const: "common.git-artifact-receiver-use.v3" }, selection: { $ref: "#/$defs/ArtifactGateSelection" }, producer: { $ref: "#/$defs/ProducerContractBinding" }, artifact: { $ref: "#/$defs/ArtifactRef" }, receiverOwnerId: { $ref: "#/$defs/Id" }, status: { enum: ["used", "rejected"] }, evidence: { type: "array", items: { $ref: "#/$defs/GitSourcePin" }, maxItems: 2048 } }, required: ["schema", "selection", "producer", "artifact", "receiverOwnerId", "status", "evidence"], additionalProperties: false };
function validate421(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.selection === undefined && (missing0 = "selection") || data.producer === undefined && (missing0 = "producer") || data.artifact === undefined && (missing0 = "artifact") || data.receiverOwnerId === undefined && (missing0 = "receiverOwnerId") || data.status === undefined && (missing0 = "status") || data.evidence === undefined && (missing0 = "evidence")) {
        validate421.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "selection" || key0 === "producer" || key0 === "artifact" || key0 === "receiverOwnerId" || key0 === "status" || key0 === "evidence")) {
            validate421.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-artifact-receiver-use.v3") {
              validate421.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-artifact-receiver-use.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.selection !== undefined) {
              const _errs3 = errors;
              if (!validate193(data.selection, { instancePath: instancePath + "/selection", parentData: data, parentDataProperty: "selection", rootData })) {
                vErrors = vErrors === null ? validate193.errors : vErrors.concat(validate193.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.producer !== undefined) {
                const _errs4 = errors;
                if (!validate181(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
                  vErrors = vErrors === null ? validate181.errors : vErrors.concat(validate181.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.artifact !== undefined) {
                  const _errs5 = errors;
                  if (!validate133(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                    vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.receiverOwnerId !== undefined) {
                    const _errs6 = errors;
                    if (!validate24(data.receiverOwnerId, { instancePath: instancePath + "/receiverOwnerId", parentData: data, parentDataProperty: "receiverOwnerId", rootData })) {
                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.status !== undefined) {
                      let data5 = data.status;
                      const _errs7 = errors;
                      if (!(data5 === "used" || data5 === "rejected")) {
                        validate421.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema72.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                        return false;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.evidence !== undefined) {
                        let data6 = data.evidence;
                        const _errs8 = errors;
                        if (errors === _errs8) {
                          if (Array.isArray(data6)) {
                            if (data6.length > 2048) {
                              validate421.errors = [{ instancePath: instancePath + "/evidence", schemaPath: "#/properties/evidence/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                              return false;
                            } else {
                              var valid1 = true;
                              const len0 = data6.length;
                              for (let i0 = 0;i0 < len0; i0++) {
                                const _errs10 = errors;
                                if (!validate58(data6[i0], { instancePath: instancePath + "/evidence/" + i0, parentData: data6, parentDataProperty: i0, rootData })) {
                                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                  errors = vErrors.length;
                                }
                                var valid1 = _errs10 === errors;
                                if (!valid1) {
                                  break;
                                }
                              }
                            }
                          } else {
                            validate421.errors = [{ instancePath: instancePath + "/evidence", schemaPath: "#/properties/evidence/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                            return false;
                          }
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate421.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate421.errors = vErrors;
  return errors === 0;
}
var validateSelectedExternalArtifact = validate427;
function validate427(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.producer === undefined && (missing0 = "producer") || data.producerContext === undefined && (missing0 = "producerContext") || data.artifact === undefined && (missing0 = "artifact") || data.artifactSource === undefined && (missing0 = "artifactSource") || data.producerMilestone === undefined && (missing0 = "producerMilestone") || data.selection === undefined && (missing0 = "selection") || data.selectionReceipt === undefined && (missing0 = "selectionReceipt") || data.criteria === undefined && (missing0 = "criteria") || data.receiverOwnerId === undefined && (missing0 = "receiverOwnerId") || data.receiverUse === undefined && (missing0 = "receiverUse") || data.producerSources === undefined && (missing0 = "producerSources") || data.consumerSources === undefined && (missing0 = "consumerSources")) {
        validate427.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema40.properties, key0)) {
            validate427.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-selected-external-artifact.v3") {
              validate427.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-selected-external-artifact.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.producer !== undefined) {
              const _errs3 = errors;
              if (!validate181(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
                vErrors = vErrors === null ? validate181.errors : vErrors.concat(validate181.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.producerContext !== undefined) {
                const _errs4 = errors;
                if (!validate58(data.producerContext, { instancePath: instancePath + "/producerContext", parentData: data, parentDataProperty: "producerContext", rootData })) {
                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.artifact !== undefined) {
                  const _errs5 = errors;
                  if (!validate133(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                    vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.artifactSource !== undefined) {
                    const _errs6 = errors;
                    if (!validate58(data.artifactSource, { instancePath: instancePath + "/artifactSource", parentData: data, parentDataProperty: "artifactSource", rootData })) {
                      vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.producerMilestone !== undefined) {
                      const _errs7 = errors;
                      if (!validate58(data.producerMilestone, { instancePath: instancePath + "/producerMilestone", parentData: data, parentDataProperty: "producerMilestone", rootData })) {
                        vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.selection !== undefined) {
                        const _errs8 = errors;
                        if (!validate193(data.selection, { instancePath: instancePath + "/selection", parentData: data, parentDataProperty: "selection", rootData })) {
                          vErrors = vErrors === null ? validate193.errors : vErrors.concat(validate193.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.selectionReceipt !== undefined) {
                          const _errs9 = errors;
                          if (!validate58(data.selectionReceipt, { instancePath: instancePath + "/selectionReceipt", parentData: data, parentDataProperty: "selectionReceipt", rootData })) {
                            vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.criteria !== undefined) {
                            let data8 = data.criteria;
                            const _errs10 = errors;
                            if (errors === _errs10) {
                              if (Array.isArray(data8)) {
                                if (data8.length > 2048) {
                                  validate427.errors = [{ instancePath: instancePath + "/criteria", schemaPath: "#/properties/criteria/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                  return false;
                                } else {
                                  var valid1 = true;
                                  const len0 = data8.length;
                                  for (let i0 = 0;i0 < len0; i0++) {
                                    let data9 = data8[i0];
                                    const _errs12 = errors;
                                    if (errors === _errs12) {
                                      if (data9 && typeof data9 == "object" && !Array.isArray(data9)) {
                                        let missing1;
                                        if (data9.criterionId === undefined && (missing1 = "criterionId") || data9.receipt === undefined && (missing1 = "receipt")) {
                                          validate427.errors = [{ instancePath: instancePath + "/criteria/" + i0, schemaPath: "#/properties/criteria/items/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                                          return false;
                                        } else {
                                          const _errs14 = errors;
                                          for (const key1 in data9) {
                                            if (!(key1 === "criterionId" || key1 === "receipt")) {
                                              validate427.errors = [{ instancePath: instancePath + "/criteria/" + i0, schemaPath: "#/properties/criteria/items/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                              return false;
                                              break;
                                            }
                                          }
                                          if (_errs14 === errors) {
                                            if (data9.criterionId !== undefined) {
                                              const _errs15 = errors;
                                              if (!validate24(data9.criterionId, { instancePath: instancePath + "/criteria/" + i0 + "/criterionId", parentData: data9, parentDataProperty: "criterionId", rootData })) {
                                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                errors = vErrors.length;
                                              }
                                              var valid2 = _errs15 === errors;
                                            } else {
                                              var valid2 = true;
                                            }
                                            if (valid2) {
                                              if (data9.receipt !== undefined) {
                                                const _errs16 = errors;
                                                if (!validate58(data9.receipt, { instancePath: instancePath + "/criteria/" + i0 + "/receipt", parentData: data9, parentDataProperty: "receipt", rootData })) {
                                                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid2 = _errs16 === errors;
                                              } else {
                                                var valid2 = true;
                                              }
                                            }
                                          }
                                        }
                                      } else {
                                        validate427.errors = [{ instancePath: instancePath + "/criteria/" + i0, schemaPath: "#/properties/criteria/items/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                        return false;
                                      }
                                    }
                                    var valid1 = _errs12 === errors;
                                    if (!valid1) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate427.errors = [{ instancePath: instancePath + "/criteria", schemaPath: "#/properties/criteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs10 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.receiverOwnerId !== undefined) {
                              const _errs17 = errors;
                              if (!validate24(data.receiverOwnerId, { instancePath: instancePath + "/receiverOwnerId", parentData: data, parentDataProperty: "receiverOwnerId", rootData })) {
                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                errors = vErrors.length;
                              }
                              var valid0 = _errs17 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.receiverUse !== undefined) {
                                const _errs18 = errors;
                                if (!validate58(data.receiverUse, { instancePath: instancePath + "/receiverUse", parentData: data, parentDataProperty: "receiverUse", rootData })) {
                                  vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                  errors = vErrors.length;
                                }
                                var valid0 = _errs18 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.producerSources !== undefined) {
                                  const _errs19 = errors;
                                  if (!validate213(data.producerSources, { instancePath: instancePath + "/producerSources", parentData: data, parentDataProperty: "producerSources", rootData })) {
                                    vErrors = vErrors === null ? validate213.errors : vErrors.concat(validate213.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid0 = _errs19 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.consumerSources !== undefined) {
                                    const _errs20 = errors;
                                    if (!validate213(data.consumerSources, { instancePath: instancePath + "/consumerSources", parentData: data, parentDataProperty: "consumerSources", rootData })) {
                                      vErrors = vErrors === null ? validate213.errors : vErrors.concat(validate213.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid0 = _errs20 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate427.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate427.errors = vErrors;
  return errors === 0;
}
var validateLegacyCreateRequestBinding = validate441;
function validate441(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.legacyOwner === undefined && (missing0 = "legacyOwner") || data.createRequestId === undefined && (missing0 = "createRequestId") || data.kind === undefined && (missing0 = "kind") || data.id === undefined && (missing0 = "id") || data.meaningSha256 === undefined && (missing0 = "meaningSha256") || data.source === undefined && (missing0 = "source")) {
        validate441.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "ownerId" || key0 === "legacyOwner" || key0 === "createRequestId" || key0 === "kind" || key0 === "id" || key0 === "meaningSha256" || key0 === "source")) {
            validate441.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-legacy-create-binding.v3") {
              validate441.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-legacy-create-binding.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.ownerId, { instancePath: instancePath + "/ownerId", parentData: data, parentDataProperty: "ownerId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.legacyOwner !== undefined) {
                let data2 = data.legacyOwner;
                const _errs4 = errors;
                if (errors === _errs4) {
                  if (data2 && typeof data2 == "object" && !Array.isArray(data2)) {
                    let missing1;
                    if (data2.repositoryId === undefined && (missing1 = "repositoryId") || data2.projectId === undefined && (missing1 = "projectId") || data2.workId === undefined && (missing1 = "workId")) {
                      validate441.errors = [{ instancePath: instancePath + "/legacyOwner", schemaPath: "#/properties/legacyOwner/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                      return false;
                    } else {
                      const _errs6 = errors;
                      for (const key1 in data2) {
                        if (!(key1 === "repositoryId" || key1 === "projectId" || key1 === "workId")) {
                          validate441.errors = [{ instancePath: instancePath + "/legacyOwner", schemaPath: "#/properties/legacyOwner/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                          return false;
                          break;
                        }
                      }
                      if (_errs6 === errors) {
                        if (data2.repositoryId !== undefined) {
                          const _errs7 = errors;
                          if (!validate24(data2.repositoryId, { instancePath: instancePath + "/legacyOwner/repositoryId", parentData: data2, parentDataProperty: "repositoryId", rootData })) {
                            vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                        } else {
                          var valid1 = true;
                        }
                        if (valid1) {
                          if (data2.projectId !== undefined) {
                            const _errs8 = errors;
                            if (!validate24(data2.projectId, { instancePath: instancePath + "/legacyOwner/projectId", parentData: data2, parentDataProperty: "projectId", rootData })) {
                              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                              errors = vErrors.length;
                            }
                            var valid1 = _errs8 === errors;
                          } else {
                            var valid1 = true;
                          }
                          if (valid1) {
                            if (data2.workId !== undefined) {
                              const _errs9 = errors;
                              if (!validate24(data2.workId, { instancePath: instancePath + "/legacyOwner/workId", parentData: data2, parentDataProperty: "workId", rootData })) {
                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                errors = vErrors.length;
                              }
                              var valid1 = _errs9 === errors;
                            } else {
                              var valid1 = true;
                            }
                          }
                        }
                      }
                    }
                  } else {
                    validate441.errors = [{ instancePath: instancePath + "/legacyOwner", schemaPath: "#/properties/legacyOwner/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.createRequestId !== undefined) {
                  const _errs10 = errors;
                  if (!validate24(data.createRequestId, { instancePath: instancePath + "/createRequestId", parentData: data, parentDataProperty: "createRequestId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs10 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.kind !== undefined) {
                    const _errs11 = errors;
                    if (data.kind !== "task") {
                      validate441.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/const", keyword: "const", params: { allowedValue: "task" }, message: "must be equal to constant" }];
                      return false;
                    }
                    var valid0 = _errs11 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.id !== undefined) {
                      const _errs12 = errors;
                      if (!validate24(data.id, { instancePath: instancePath + "/id", parentData: data, parentDataProperty: "id", rootData })) {
                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs12 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.meaningSha256 !== undefined) {
                        const _errs13 = errors;
                        if (!validate52(data.meaningSha256, { instancePath: instancePath + "/meaningSha256", parentData: data, parentDataProperty: "meaningSha256", rootData })) {
                          vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs13 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.source !== undefined) {
                          const _errs14 = errors;
                          if (!validate58(data.source, { instancePath: instancePath + "/source", parentData: data, parentDataProperty: "source", rootData })) {
                            vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs14 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate441.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate441.errors = vErrors;
  return errors === 0;
}
var validateLegacyMigrationRequest = validate450;
function validate453(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.repositoryId === undefined && (missing0 = "repositoryId") || data.commit === undefined && (missing0 = "commit") || data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId")) {
        validate453.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "repositoryId" || key0 === "commit" || key0 === "projectId" || key0 === "workId")) {
            validate453.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.repositoryId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.repositoryId, { instancePath: instancePath + "/repositoryId", parentData: data, parentDataProperty: "repositoryId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.commit !== undefined) {
              let data1 = data.commit;
              const _errs3 = errors;
              if (errors === _errs3) {
                if (typeof data1 === "string") {
                  if (!pattern2.test(data1)) {
                    validate453.errors = [{ instancePath: instancePath + "/commit", schemaPath: "#/properties/commit/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{40}$" }, message: 'must match pattern "' + "^[a-f0-9]{40}$" + '"' }];
                    return false;
                  }
                } else {
                  validate453.errors = [{ instancePath: instancePath + "/commit", schemaPath: "#/properties/commit/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.projectId !== undefined) {
                const _errs5 = errors;
                if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.workId !== undefined) {
                  const _errs6 = errors;
                  if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs6 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate453.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate453.errors = vErrors;
  return errors === 0;
}
function validate450(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.createRequestId === undefined && (missing0 = "createRequestId") || data.snapshot === undefined && (missing0 = "snapshot") || data.expectedInventoryDigest === undefined && (missing0 = "expectedInventoryDigest") || data.bootstrapRequests === undefined && (missing0 = "bootstrapRequests") || data.aliases === undefined && (missing0 = "aliases")) {
        validate450.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "ownerId" || key0 === "createRequestId" || key0 === "snapshot" || key0 === "expectedInventoryDigest" || key0 === "bootstrapRequests" || key0 === "aliases")) {
            validate450.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-legacy-migration-request.v3") {
              validate450.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-legacy-migration-request.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.ownerId, { instancePath: instancePath + "/ownerId", parentData: data, parentDataProperty: "ownerId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.createRequestId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.createRequestId, { instancePath: instancePath + "/createRequestId", parentData: data, parentDataProperty: "createRequestId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.snapshot !== undefined) {
                  const _errs5 = errors;
                  if (!validate453(data.snapshot, { instancePath: instancePath + "/snapshot", parentData: data, parentDataProperty: "snapshot", rootData })) {
                    vErrors = vErrors === null ? validate453.errors : vErrors.concat(validate453.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.expectedInventoryDigest !== undefined) {
                    const _errs6 = errors;
                    if (!validate52(data.expectedInventoryDigest, { instancePath: instancePath + "/expectedInventoryDigest", parentData: data, parentDataProperty: "expectedInventoryDigest", rootData })) {
                      vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.bootstrapRequests !== undefined) {
                      let data5 = data.bootstrapRequests;
                      const _errs7 = errors;
                      if (errors === _errs7) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2048) {
                            validate450.errors = [{ instancePath: instancePath + "/bootstrapRequests", schemaPath: "#/properties/bootstrapRequests/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                            return false;
                          } else {
                            var valid1 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs9 = errors;
                              if (!validate58(data5[i0], { instancePath: instancePath + "/bootstrapRequests/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                errors = vErrors.length;
                              }
                              var valid1 = _errs9 === errors;
                              if (!valid1) {
                                break;
                              }
                            }
                          }
                        } else {
                          validate450.errors = [{ instancePath: instancePath + "/bootstrapRequests", schemaPath: "#/properties/bootstrapRequests/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                          return false;
                        }
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.aliases !== undefined) {
                        let data7 = data.aliases;
                        const _errs10 = errors;
                        if (errors === _errs10) {
                          if (Array.isArray(data7)) {
                            if (data7.length > 2048) {
                              validate450.errors = [{ instancePath: instancePath + "/aliases", schemaPath: "#/properties/aliases/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                              return false;
                            } else {
                              var valid2 = true;
                              const len1 = data7.length;
                              for (let i1 = 0;i1 < len1; i1++) {
                                const _errs12 = errors;
                                if (!validate171(data7[i1], { instancePath: instancePath + "/aliases/" + i1, parentData: data7, parentDataProperty: i1, rootData })) {
                                  vErrors = vErrors === null ? validate171.errors : vErrors.concat(validate171.errors);
                                  errors = vErrors.length;
                                }
                                var valid2 = _errs12 === errors;
                                if (!valid2) {
                                  break;
                                }
                              }
                            }
                          } else {
                            validate450.errors = [{ instancePath: instancePath + "/aliases", schemaPath: "#/properties/aliases/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                            return false;
                          }
                        }
                        var valid0 = _errs10 === errors;
                      } else {
                        var valid0 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate450.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate450.errors = vErrors;
  return errors === 0;
}
var validateLegacyWriterFence = validate461;
function validate463(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId")) {
        validate463.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId")) {
            validate463.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.projectId, { instancePath: instancePath + "/projectId", parentData: data, parentDataProperty: "projectId", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.workId, { instancePath: instancePath + "/workId", parentData: data, parentDataProperty: "workId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate463.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate463.errors = vErrors;
  return errors === 0;
}
function validate461(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.scope === undefined && (missing0 = "scope") || data.authoritativeProtocol === undefined && (missing0 = "authoritativeProtocol") || data.legacyReadSnapshot === undefined && (missing0 = "legacyReadSnapshot") || data.inventoryDigest === undefined && (missing0 = "inventoryDigest") || data.migrationRequestDigest === undefined && (missing0 = "migrationRequestDigest") || data.sources === undefined && (missing0 = "sources")) {
        validate461.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "ownerId" || key0 === "scope" || key0 === "authoritativeProtocol" || key0 === "legacyReadSnapshot" || key0 === "inventoryDigest" || key0 === "migrationRequestDigest" || key0 === "sources")) {
            validate461.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-writer-fence.v3") {
              validate461.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-writer-fence.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.ownerId, { instancePath: instancePath + "/ownerId", parentData: data, parentDataProperty: "ownerId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.scope !== undefined) {
                const _errs4 = errors;
                if (!validate463(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData })) {
                  vErrors = vErrors === null ? validate463.errors : vErrors.concat(validate463.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.authoritativeProtocol !== undefined) {
                  const _errs5 = errors;
                  if (data.authoritativeProtocol !== "v3") {
                    validate461.errors = [{ instancePath: instancePath + "/authoritativeProtocol", schemaPath: "#/properties/authoritativeProtocol/const", keyword: "const", params: { allowedValue: "v3" }, message: "must be equal to constant" }];
                    return false;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.legacyReadSnapshot !== undefined) {
                    const _errs6 = errors;
                    if (!validate453(data.legacyReadSnapshot, { instancePath: instancePath + "/legacyReadSnapshot", parentData: data, parentDataProperty: "legacyReadSnapshot", rootData })) {
                      vErrors = vErrors === null ? validate453.errors : vErrors.concat(validate453.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.inventoryDigest !== undefined) {
                      const _errs7 = errors;
                      if (!validate52(data.inventoryDigest, { instancePath: instancePath + "/inventoryDigest", parentData: data, parentDataProperty: "inventoryDigest", rootData })) {
                        vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.migrationRequestDigest !== undefined) {
                        const _errs8 = errors;
                        if (!validate52(data.migrationRequestDigest, { instancePath: instancePath + "/migrationRequestDigest", parentData: data, parentDataProperty: "migrationRequestDigest", rootData })) {
                          vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.sources !== undefined) {
                          let data7 = data.sources;
                          const _errs9 = errors;
                          if (errors === _errs9) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 1e4) {
                                validate461.errors = [{ instancePath: instancePath + "/sources", schemaPath: "#/properties/sources/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                return false;
                              } else {
                                var valid1 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs11 = errors;
                                  if (!validate58(data7[i0], { instancePath: instancePath + "/sources/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid1 = _errs11 === errors;
                                  if (!valid1) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate461.errors = [{ instancePath: instancePath + "/sources", schemaPath: "#/properties/sources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate461.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate461.errors = vErrors;
  return errors === 0;
}
var validateLegacyMigrationReceipt = validate471;
var schema79 = { type: "object", properties: { schema: { const: "common.git-legacy-migration-receipt.v3" }, ownerId: { $ref: "#/$defs/Id" }, createRequestId: { $ref: "#/$defs/Id" }, requestDigest: { $ref: "#/$defs/Digest" }, scope: { $ref: "#/$defs/WorkRef" }, legacySnapshot: { $ref: "#/$defs/SnapshotIdentity" }, inventoryDigest: { $ref: "#/$defs/Digest" }, preservedSources: { type: "array", items: { $ref: "#/$defs/GitSourcePin" }, maxItems: 1e4 }, projectedFiles: { type: "array", items: { type: "object", properties: { path: { type: "string", minLength: 1, maxLength: 4096 }, blobOid: { type: "string", pattern: "^[a-f0-9]{40}$" }, sha256: { $ref: "#/$defs/Digest" } }, required: ["path", "blobOid", "sha256"], additionalProperties: false }, maxItems: 1e4 }, writerFencePath: { type: "string", minLength: 1, maxLength: 4096 }, legacyRequestPaths: { type: "array", items: { type: "string", minLength: 1, maxLength: 4096 }, maxItems: 2048 }, aliases: { type: "array", items: { $ref: "#/$defs/GitWorkAlias" }, maxItems: 2048 }, authority: { type: "object", properties: { actorId: { $ref: "#/$defs/Id" }, revision: { $ref: "#/$defs/Revision" }, authorityRef: { $ref: "#/$defs/Id" }, gitHookPolicyRef: { $ref: "#/$defs/Id" } }, required: ["actorId", "revision", "authorityRef", "gitHookPolicyRef"], additionalProperties: false } }, required: ["schema", "ownerId", "createRequestId", "requestDigest", "scope", "legacySnapshot", "inventoryDigest", "preservedSources", "projectedFiles", "writerFencePath", "legacyRequestPaths", "aliases", "authority"], additionalProperties: false };
function validate471(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.createRequestId === undefined && (missing0 = "createRequestId") || data.requestDigest === undefined && (missing0 = "requestDigest") || data.scope === undefined && (missing0 = "scope") || data.legacySnapshot === undefined && (missing0 = "legacySnapshot") || data.inventoryDigest === undefined && (missing0 = "inventoryDigest") || data.preservedSources === undefined && (missing0 = "preservedSources") || data.projectedFiles === undefined && (missing0 = "projectedFiles") || data.writerFencePath === undefined && (missing0 = "writerFencePath") || data.legacyRequestPaths === undefined && (missing0 = "legacyRequestPaths") || data.aliases === undefined && (missing0 = "aliases") || data.authority === undefined && (missing0 = "authority")) {
        validate471.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema79.properties, key0)) {
            validate471.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-legacy-migration-receipt.v3") {
              validate471.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-legacy-migration-receipt.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.ownerId, { instancePath: instancePath + "/ownerId", parentData: data, parentDataProperty: "ownerId", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.createRequestId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.createRequestId, { instancePath: instancePath + "/createRequestId", parentData: data, parentDataProperty: "createRequestId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.requestDigest !== undefined) {
                  const _errs5 = errors;
                  if (!validate52(data.requestDigest, { instancePath: instancePath + "/requestDigest", parentData: data, parentDataProperty: "requestDigest", rootData })) {
                    vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.scope !== undefined) {
                    const _errs6 = errors;
                    if (!validate463(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData })) {
                      vErrors = vErrors === null ? validate463.errors : vErrors.concat(validate463.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.legacySnapshot !== undefined) {
                      const _errs7 = errors;
                      if (!validate453(data.legacySnapshot, { instancePath: instancePath + "/legacySnapshot", parentData: data, parentDataProperty: "legacySnapshot", rootData })) {
                        vErrors = vErrors === null ? validate453.errors : vErrors.concat(validate453.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.inventoryDigest !== undefined) {
                        const _errs8 = errors;
                        if (!validate52(data.inventoryDigest, { instancePath: instancePath + "/inventoryDigest", parentData: data, parentDataProperty: "inventoryDigest", rootData })) {
                          vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.preservedSources !== undefined) {
                          let data7 = data.preservedSources;
                          const _errs9 = errors;
                          if (errors === _errs9) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 1e4) {
                                validate471.errors = [{ instancePath: instancePath + "/preservedSources", schemaPath: "#/properties/preservedSources/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                return false;
                              } else {
                                var valid1 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs11 = errors;
                                  if (!validate58(data7[i0], { instancePath: instancePath + "/preservedSources/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid1 = _errs11 === errors;
                                  if (!valid1) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate471.errors = [{ instancePath: instancePath + "/preservedSources", schemaPath: "#/properties/preservedSources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs9 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.projectedFiles !== undefined) {
                            let data9 = data.projectedFiles;
                            const _errs12 = errors;
                            if (errors === _errs12) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 1e4) {
                                  validate471.errors = [{ instancePath: instancePath + "/projectedFiles", schemaPath: "#/properties/projectedFiles/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    let data10 = data9[i1];
                                    const _errs14 = errors;
                                    if (errors === _errs14) {
                                      if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                                        let missing1;
                                        if (data10.path === undefined && (missing1 = "path") || data10.blobOid === undefined && (missing1 = "blobOid") || data10.sha256 === undefined && (missing1 = "sha256")) {
                                          validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1, schemaPath: "#/properties/projectedFiles/items/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                                          return false;
                                        } else {
                                          const _errs16 = errors;
                                          for (const key1 in data10) {
                                            if (!(key1 === "path" || key1 === "blobOid" || key1 === "sha256")) {
                                              validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1, schemaPath: "#/properties/projectedFiles/items/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                              return false;
                                              break;
                                            }
                                          }
                                          if (_errs16 === errors) {
                                            if (data10.path !== undefined) {
                                              let data11 = data10.path;
                                              const _errs17 = errors;
                                              if (errors === _errs17) {
                                                if (typeof data11 === "string") {
                                                  if (func3(data11) > 4096) {
                                                    validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1 + "/path", schemaPath: "#/properties/projectedFiles/items/properties/path/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                                    return false;
                                                  } else {
                                                    if (func3(data11) < 1) {
                                                      validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1 + "/path", schemaPath: "#/properties/projectedFiles/items/properties/path/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                                      return false;
                                                    }
                                                  }
                                                } else {
                                                  validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1 + "/path", schemaPath: "#/properties/projectedFiles/items/properties/path/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                  return false;
                                                }
                                              }
                                              var valid3 = _errs17 === errors;
                                            } else {
                                              var valid3 = true;
                                            }
                                            if (valid3) {
                                              if (data10.blobOid !== undefined) {
                                                let data12 = data10.blobOid;
                                                const _errs19 = errors;
                                                if (errors === _errs19) {
                                                  if (typeof data12 === "string") {
                                                    if (!pattern2.test(data12)) {
                                                      validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1 + "/blobOid", schemaPath: "#/properties/projectedFiles/items/properties/blobOid/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{40}$" }, message: 'must match pattern "' + "^[a-f0-9]{40}$" + '"' }];
                                                      return false;
                                                    }
                                                  } else {
                                                    validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1 + "/blobOid", schemaPath: "#/properties/projectedFiles/items/properties/blobOid/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                    return false;
                                                  }
                                                }
                                                var valid3 = _errs19 === errors;
                                              } else {
                                                var valid3 = true;
                                              }
                                              if (valid3) {
                                                if (data10.sha256 !== undefined) {
                                                  const _errs21 = errors;
                                                  if (!validate52(data10.sha256, { instancePath: instancePath + "/projectedFiles/" + i1 + "/sha256", parentData: data10, parentDataProperty: "sha256", rootData })) {
                                                    vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                                                    errors = vErrors.length;
                                                  }
                                                  var valid3 = _errs21 === errors;
                                                } else {
                                                  var valid3 = true;
                                                }
                                              }
                                            }
                                          }
                                        }
                                      } else {
                                        validate471.errors = [{ instancePath: instancePath + "/projectedFiles/" + i1, schemaPath: "#/properties/projectedFiles/items/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                        return false;
                                      }
                                    }
                                    var valid2 = _errs14 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate471.errors = [{ instancePath: instancePath + "/projectedFiles", schemaPath: "#/properties/projectedFiles/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs12 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.writerFencePath !== undefined) {
                              let data14 = data.writerFencePath;
                              const _errs22 = errors;
                              if (errors === _errs22) {
                                if (typeof data14 === "string") {
                                  if (func3(data14) > 4096) {
                                    validate471.errors = [{ instancePath: instancePath + "/writerFencePath", schemaPath: "#/properties/writerFencePath/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                    return false;
                                  } else {
                                    if (func3(data14) < 1) {
                                      validate471.errors = [{ instancePath: instancePath + "/writerFencePath", schemaPath: "#/properties/writerFencePath/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                      return false;
                                    }
                                  }
                                } else {
                                  validate471.errors = [{ instancePath: instancePath + "/writerFencePath", schemaPath: "#/properties/writerFencePath/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs22 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.legacyRequestPaths !== undefined) {
                                let data15 = data.legacyRequestPaths;
                                const _errs24 = errors;
                                if (errors === _errs24) {
                                  if (Array.isArray(data15)) {
                                    if (data15.length > 2048) {
                                      validate471.errors = [{ instancePath: instancePath + "/legacyRequestPaths", schemaPath: "#/properties/legacyRequestPaths/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                      return false;
                                    } else {
                                      var valid4 = true;
                                      const len2 = data15.length;
                                      for (let i2 = 0;i2 < len2; i2++) {
                                        let data16 = data15[i2];
                                        const _errs26 = errors;
                                        if (errors === _errs26) {
                                          if (typeof data16 === "string") {
                                            if (func3(data16) > 4096) {
                                              validate471.errors = [{ instancePath: instancePath + "/legacyRequestPaths/" + i2, schemaPath: "#/properties/legacyRequestPaths/items/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                              return false;
                                            } else {
                                              if (func3(data16) < 1) {
                                                validate471.errors = [{ instancePath: instancePath + "/legacyRequestPaths/" + i2, schemaPath: "#/properties/legacyRequestPaths/items/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                                return false;
                                              }
                                            }
                                          } else {
                                            validate471.errors = [{ instancePath: instancePath + "/legacyRequestPaths/" + i2, schemaPath: "#/properties/legacyRequestPaths/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                            return false;
                                          }
                                        }
                                        var valid4 = _errs26 === errors;
                                        if (!valid4) {
                                          break;
                                        }
                                      }
                                    }
                                  } else {
                                    validate471.errors = [{ instancePath: instancePath + "/legacyRequestPaths", schemaPath: "#/properties/legacyRequestPaths/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs24 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.aliases !== undefined) {
                                  let data17 = data.aliases;
                                  const _errs28 = errors;
                                  if (errors === _errs28) {
                                    if (Array.isArray(data17)) {
                                      if (data17.length > 2048) {
                                        validate471.errors = [{ instancePath: instancePath + "/aliases", schemaPath: "#/properties/aliases/maxItems", keyword: "maxItems", params: { limit: 2048 }, message: "must NOT have more than 2048 items" }];
                                        return false;
                                      } else {
                                        var valid5 = true;
                                        const len3 = data17.length;
                                        for (let i3 = 0;i3 < len3; i3++) {
                                          const _errs30 = errors;
                                          if (!validate171(data17[i3], { instancePath: instancePath + "/aliases/" + i3, parentData: data17, parentDataProperty: i3, rootData })) {
                                            vErrors = vErrors === null ? validate171.errors : vErrors.concat(validate171.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid5 = _errs30 === errors;
                                          if (!valid5) {
                                            break;
                                          }
                                        }
                                      }
                                    } else {
                                      validate471.errors = [{ instancePath: instancePath + "/aliases", schemaPath: "#/properties/aliases/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs28 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.authority !== undefined) {
                                    let data19 = data.authority;
                                    const _errs31 = errors;
                                    if (errors === _errs31) {
                                      if (data19 && typeof data19 == "object" && !Array.isArray(data19)) {
                                        let missing2;
                                        if (data19.actorId === undefined && (missing2 = "actorId") || data19.revision === undefined && (missing2 = "revision") || data19.authorityRef === undefined && (missing2 = "authorityRef") || data19.gitHookPolicyRef === undefined && (missing2 = "gitHookPolicyRef")) {
                                          validate471.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                                          return false;
                                        } else {
                                          const _errs33 = errors;
                                          for (const key2 in data19) {
                                            if (!(key2 === "actorId" || key2 === "revision" || key2 === "authorityRef" || key2 === "gitHookPolicyRef")) {
                                              validate471.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                              return false;
                                              break;
                                            }
                                          }
                                          if (_errs33 === errors) {
                                            if (data19.actorId !== undefined) {
                                              const _errs34 = errors;
                                              if (!validate24(data19.actorId, { instancePath: instancePath + "/authority/actorId", parentData: data19, parentDataProperty: "actorId", rootData })) {
                                                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                errors = vErrors.length;
                                              }
                                              var valid6 = _errs34 === errors;
                                            } else {
                                              var valid6 = true;
                                            }
                                            if (valid6) {
                                              if (data19.revision !== undefined) {
                                                const _errs35 = errors;
                                                if (!validate39(data19.revision, { instancePath: instancePath + "/authority/revision", parentData: data19, parentDataProperty: "revision", rootData })) {
                                                  vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid6 = _errs35 === errors;
                                              } else {
                                                var valid6 = true;
                                              }
                                              if (valid6) {
                                                if (data19.authorityRef !== undefined) {
                                                  const _errs36 = errors;
                                                  if (!validate24(data19.authorityRef, { instancePath: instancePath + "/authority/authorityRef", parentData: data19, parentDataProperty: "authorityRef", rootData })) {
                                                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                    errors = vErrors.length;
                                                  }
                                                  var valid6 = _errs36 === errors;
                                                } else {
                                                  var valid6 = true;
                                                }
                                                if (valid6) {
                                                  if (data19.gitHookPolicyRef !== undefined) {
                                                    const _errs37 = errors;
                                                    if (!validate24(data19.gitHookPolicyRef, { instancePath: instancePath + "/authority/gitHookPolicyRef", parentData: data19, parentDataProperty: "gitHookPolicyRef", rootData })) {
                                                      vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                                      errors = vErrors.length;
                                                    }
                                                    var valid6 = _errs37 === errors;
                                                  } else {
                                                    var valid6 = true;
                                                  }
                                                }
                                              }
                                            }
                                          }
                                        }
                                      } else {
                                        validate471.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                        return false;
                                      }
                                    }
                                    var valid0 = _errs31 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate471.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate471.errors = vErrors;
  return errors === 0;
}
export {
  validateAllocationBinding,
  validateArtifactAcceptanceDecision,
  validateArtifactCriterionReceipt,
  validateArtifactProducerMilestone,
  validateArtifactReceiverUse,
  validateArtifactSelectionReceipt,
  validateCompactContextPacket,
  validateConsumerContractBinding,
  validateContextPacket,
  validateContract,
  validateCreateRequest,
  validateEffectiveContract,
  validateGitSourcePin,
  validateGitWorkAlias,
  validateGraph,
  validateLegacyCreateRequestBinding,
  validateLegacyMigrationReceipt,
  validateLegacyMigrationRequest,
  validateLegacyTaskBinding,
  validateLegacyWriterFence,
  validateProducerContractBinding,
  validateRun,
  validateSelectedArtifactDocument,
  validateSelectedArtifactGate,
  validateSelectedExternalArtifact,
  validateStoredRecord,
  validateTask,
  validateWork
};
