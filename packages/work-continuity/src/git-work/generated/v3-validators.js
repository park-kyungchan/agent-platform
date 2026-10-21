/** Generated from git-work-v3.schema.json; do not edit. SHA-256: 85626ea6e2a057dabd00dd2db711fad4b0b9ef5f6e5991b00eb0394850f11553 */
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
var validateGraph = validate88;
var schema12 = { type: "object", properties: { schema: { const: "common.git-work.v3" }, domains: { type: "array", items: { $ref: "#/$defs/Domain" }, maxItems: 2000 }, projects: { type: "array", items: { $ref: "#/$defs/Project" }, maxItems: 2000 }, works: { type: "array", items: { $ref: "#/$defs/Work" }, maxItems: 2000 }, tasks: { type: "array", items: { $ref: "#/$defs/Task" }, maxItems: 2000 }, contracts: { type: "array", items: { $ref: "#/$defs/Contract" }, maxItems: 1e4 }, dependencies: { type: "array", items: { $ref: "#/$defs/Dependency" }, maxItems: 1e4 }, relations: { type: "array", items: { $ref: "#/$defs/Related" }, maxItems: 1e4 }, delegations: { type: "array", items: { $ref: "#/$defs/Delegation" }, maxItems: 1e4 }, provenance: { type: "array", items: { $ref: "#/$defs/Provenance" }, maxItems: 1e4 }, nativeCorrelations: { type: "array", items: { $ref: "#/$defs/NativeCorrelation" }, maxItems: 1e4 } }, required: ["schema", "domains", "projects", "works", "tasks", "contracts", "dependencies", "relations", "delegations", "provenance", "nativeCorrelations"], additionalProperties: false };
var func2 = Object.prototype.hasOwnProperty;
var func3 = require_ucs2length().default;
var pattern0 = new RegExp("^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$", "u");
function validate12(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.title === undefined && (missing0 = "title")) {
        validate12.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "title" || key0 === "parentDomainId")) {
            validate12.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            let data0 = data.id;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate12.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate12.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate12.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate12.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.title !== undefined) {
              let data1 = data.title;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate12.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate12.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate12.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate12.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.parentDomainId !== undefined) {
                let data2 = data.parentDomainId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate12.errors = [{ instancePath: instancePath + "/parentDomainId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate12.errors = [{ instancePath: instancePath + "/parentDomainId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate12.errors = [{ instancePath: instancePath + "/parentDomainId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate12.errors = [{ instancePath: instancePath + "/parentDomainId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
    } else {
      validate12.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate12.errors = vErrors;
  return errors === 0;
}
function validate14(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.title === undefined && (missing0 = "title") || data.primaryDomainId === undefined && (missing0 = "primaryDomainId")) {
        validate14.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "title" || key0 === "primaryDomainId")) {
            validate14.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            let data0 = data.id;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate14.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate14.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate14.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate14.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.title !== undefined) {
              let data1 = data.title;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate14.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate14.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate14.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate14.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.primaryDomainId !== undefined) {
                let data2 = data.primaryDomainId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate14.errors = [{ instancePath: instancePath + "/primaryDomainId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate14.errors = [{ instancePath: instancePath + "/primaryDomainId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate14.errors = [{ instancePath: instancePath + "/primaryDomainId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate14.errors = [{ instancePath: instancePath + "/primaryDomainId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
    } else {
      validate14.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate14.errors = vErrors;
  return errors === 0;
}
var schema21 = { type: "object", properties: { projectId: { $ref: "#/$defs/Id" }, workId: { $ref: "#/$defs/Id" }, title: { $ref: "#/$defs/Id" }, leadActorId: { $ref: "#/$defs/Id" }, revision: { $ref: "#/$defs/Revision" }, contracts: { $ref: "#/$defs/ContractSet" }, status: { enum: ["proposed", "admitted", "active", "verified", "accepted", "cancelled"] } }, required: ["projectId", "workId", "title", "leadActorId", "revision", "contracts", "status"], additionalProperties: false };
var schema28 = { type: "object", properties: { id: { $ref: "#/$defs/Id" }, owner: { $ref: "#/$defs/ContractOwner" }, revision: { $ref: "#/$defs/Revision" }, kind: { enum: ["issue", "spec", "plan"] }, digest: { $ref: "#/$defs/Digest" } }, required: ["id", "owner", "revision", "kind", "digest"], additionalProperties: false };
var pattern16 = new RegExp("^[a-f0-9]{64}$", "u");
function validate19(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
              let data1 = data.projectId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    const err3 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                    if (vErrors === null) {
                      vErrors = [err3];
                    } else {
                      vErrors.push(err3);
                    }
                    errors++;
                  } else {
                    if (func3(data1) < 1) {
                      const err4 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                      if (vErrors === null) {
                        vErrors = [err4];
                      } else {
                        vErrors.push(err4);
                      }
                      errors++;
                    } else {
                      if (!pattern0.test(data1)) {
                        const err5 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                        if (vErrors === null) {
                          vErrors = [err5];
                        } else {
                          vErrors.push(err5);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err6 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err6];
                  } else {
                    vErrors.push(err6);
                  }
                  errors++;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.workId !== undefined) {
                let data2 = data.workId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      const err7 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                      if (vErrors === null) {
                        vErrors = [err7];
                      } else {
                        vErrors.push(err7);
                      }
                      errors++;
                    } else {
                      if (func3(data2) < 1) {
                        const err8 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                        if (vErrors === null) {
                          vErrors = [err8];
                        } else {
                          vErrors.push(err8);
                        }
                        errors++;
                      } else {
                        if (!pattern0.test(data2)) {
                          const err9 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err9];
                          } else {
                            vErrors.push(err9);
                          }
                          errors++;
                        }
                      }
                    }
                  } else {
                    const err10 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                    if (vErrors === null) {
                      vErrors = [err10];
                    } else {
                      vErrors.push(err10);
                    }
                    errors++;
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      }
    } else {
      const err11 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err11];
      } else {
        vErrors.push(err11);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs11 = errors;
  if (errors === _errs11) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.kind === undefined && (missing1 = "kind") || data.projectId === undefined && (missing1 = "projectId") || data.workId === undefined && (missing1 = "workId") || data.taskId === undefined && (missing1 = "taskId")) {
        const err12 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err12];
        } else {
          vErrors.push(err12);
        }
        errors++;
      } else {
        const _errs13 = errors;
        for (const key1 in data) {
          if (!(key1 === "kind" || key1 === "projectId" || key1 === "workId" || key1 === "taskId")) {
            const err13 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err13];
            } else {
              vErrors.push(err13);
            }
            errors++;
            break;
          }
        }
        if (_errs13 === errors) {
          if (data.kind !== undefined) {
            const _errs14 = errors;
            if (data.kind !== "task") {
              const err14 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "task" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err14];
              } else {
                vErrors.push(err14);
              }
              errors++;
            }
            var valid4 = _errs14 === errors;
          } else {
            var valid4 = true;
          }
          if (valid4) {
            if (data.projectId !== undefined) {
              let data4 = data.projectId;
              const _errs15 = errors;
              const _errs16 = errors;
              if (errors === _errs16) {
                if (typeof data4 === "string") {
                  if (func3(data4) > 240) {
                    const err15 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                    if (vErrors === null) {
                      vErrors = [err15];
                    } else {
                      vErrors.push(err15);
                    }
                    errors++;
                  } else {
                    if (func3(data4) < 1) {
                      const err16 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                      if (vErrors === null) {
                        vErrors = [err16];
                      } else {
                        vErrors.push(err16);
                      }
                      errors++;
                    } else {
                      if (!pattern0.test(data4)) {
                        const err17 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                        if (vErrors === null) {
                          vErrors = [err17];
                        } else {
                          vErrors.push(err17);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err18 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err18];
                  } else {
                    vErrors.push(err18);
                  }
                  errors++;
                }
              }
              var valid4 = _errs15 === errors;
            } else {
              var valid4 = true;
            }
            if (valid4) {
              if (data.workId !== undefined) {
                let data5 = data.workId;
                const _errs18 = errors;
                const _errs19 = errors;
                if (errors === _errs19) {
                  if (typeof data5 === "string") {
                    if (func3(data5) > 240) {
                      const err19 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                      if (vErrors === null) {
                        vErrors = [err19];
                      } else {
                        vErrors.push(err19);
                      }
                      errors++;
                    } else {
                      if (func3(data5) < 1) {
                        const err20 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                        if (vErrors === null) {
                          vErrors = [err20];
                        } else {
                          vErrors.push(err20);
                        }
                        errors++;
                      } else {
                        if (!pattern0.test(data5)) {
                          const err21 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err21];
                          } else {
                            vErrors.push(err21);
                          }
                          errors++;
                        }
                      }
                    }
                  } else {
                    const err22 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                    if (vErrors === null) {
                      vErrors = [err22];
                    } else {
                      vErrors.push(err22);
                    }
                    errors++;
                  }
                }
                var valid4 = _errs18 === errors;
              } else {
                var valid4 = true;
              }
              if (valid4) {
                if (data.taskId !== undefined) {
                  let data6 = data.taskId;
                  const _errs21 = errors;
                  const _errs22 = errors;
                  if (errors === _errs22) {
                    if (typeof data6 === "string") {
                      if (func3(data6) > 240) {
                        const err23 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                        if (vErrors === null) {
                          vErrors = [err23];
                        } else {
                          vErrors.push(err23);
                        }
                        errors++;
                      } else {
                        if (func3(data6) < 1) {
                          const err24 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                          if (vErrors === null) {
                            vErrors = [err24];
                          } else {
                            vErrors.push(err24);
                          }
                          errors++;
                        } else {
                          if (!pattern0.test(data6)) {
                            const err25 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                            if (vErrors === null) {
                              vErrors = [err25];
                            } else {
                              vErrors.push(err25);
                            }
                            errors++;
                          }
                        }
                      }
                    } else {
                      const err26 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                      if (vErrors === null) {
                        vErrors = [err26];
                      } else {
                        vErrors.push(err26);
                      }
                      errors++;
                    }
                  }
                  var valid4 = _errs21 === errors;
                } else {
                  var valid4 = true;
                }
              }
            }
          }
        }
      }
    } else {
      const err27 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err27];
      } else {
        vErrors.push(err27);
      }
      errors++;
    }
  }
  var _valid0 = _errs11 === errors;
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
    const err28 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err28];
    } else {
      vErrors.push(err28);
    }
    errors++;
    validate19.errors = vErrors;
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
  validate19.errors = vErrors;
  return errors === 0;
}
function validate18(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.owner === undefined && (missing0 = "owner") || data.revision === undefined && (missing0 = "revision") || data.kind === undefined && (missing0 = "kind") || data.digest === undefined && (missing0 = "digest")) {
        validate18.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "owner" || key0 === "revision" || key0 === "kind" || key0 === "digest")) {
            validate18.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            let data0 = data.id;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate18.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate18.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate18.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate18.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.owner !== undefined) {
              const _errs5 = errors;
              if (!validate19(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate19.errors : vErrors.concat(validate19.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.revision !== undefined) {
                let data2 = data.revision;
                const _errs6 = errors;
                const _errs7 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  validate18.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                  return false;
                }
                if (errors === _errs7) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      validate18.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                      return false;
                    } else {
                      if (data2 < 1 || isNaN(data2)) {
                        validate18.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                        return false;
                      }
                    }
                  }
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.kind !== undefined) {
                  let data3 = data.kind;
                  const _errs9 = errors;
                  if (!(data3 === "issue" || data3 === "spec" || data3 === "plan")) {
                    validate18.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema28.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs9 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.digest !== undefined) {
                    let data4 = data.digest;
                    const _errs10 = errors;
                    const _errs11 = errors;
                    if (errors === _errs11) {
                      if (typeof data4 === "string") {
                        if (!pattern16.test(data4)) {
                          validate18.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                          return false;
                        }
                      } else {
                        validate18.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
    } else {
      validate18.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate18.errors = vErrors;
  return errors === 0;
}
function validate17(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.issue === undefined && (missing0 = "issue") || data.spec === undefined && (missing0 = "spec") || data.plan === undefined && (missing0 = "plan")) {
        validate17.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "issue" || key0 === "spec" || key0 === "plan")) {
            validate17.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.issue !== undefined) {
            const _errs2 = errors;
            if (!validate18(data.issue, { instancePath: instancePath + "/issue", parentData: data, parentDataProperty: "issue", rootData })) {
              vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.spec !== undefined) {
              const _errs3 = errors;
              if (!validate18(data.spec, { instancePath: instancePath + "/spec", parentData: data, parentDataProperty: "spec", rootData })) {
                vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.plan !== undefined) {
                const _errs4 = errors;
                if (!validate18(data.plan, { instancePath: instancePath + "/plan", parentData: data, parentDataProperty: "plan", rootData })) {
                  vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
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
      validate17.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate17.errors = vErrors;
  return errors === 0;
}
function validate16(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.title === undefined && (missing0 = "title") || data.leadActorId === undefined && (missing0 = "leadActorId") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts") || data.status === undefined && (missing0 = "status")) {
        validate16.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId" || key0 === "title" || key0 === "leadActorId" || key0 === "revision" || key0 === "contracts" || key0 === "status")) {
            validate16.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            let data0 = data.projectId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate16.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate16.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate16.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate16.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              let data1 = data.workId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate16.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate16.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate16.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate16.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.title !== undefined) {
                let data2 = data.title;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate16.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate16.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate16.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate16.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs8 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.leadActorId !== undefined) {
                  let data3 = data.leadActorId;
                  const _errs11 = errors;
                  const _errs12 = errors;
                  if (errors === _errs12) {
                    if (typeof data3 === "string") {
                      if (func3(data3) > 240) {
                        validate16.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                        return false;
                      } else {
                        if (func3(data3) < 1) {
                          validate16.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        } else {
                          if (!pattern0.test(data3)) {
                            validate16.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                            return false;
                          }
                        }
                      }
                    } else {
                      validate16.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs11 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.revision !== undefined) {
                    let data4 = data.revision;
                    const _errs14 = errors;
                    const _errs15 = errors;
                    if (!(typeof data4 == "number" && (!(data4 % 1) && !isNaN(data4)) && isFinite(data4))) {
                      validate16.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                      return false;
                    }
                    if (errors === _errs15) {
                      if (typeof data4 == "number" && isFinite(data4)) {
                        if (data4 > 9007199254740991 || isNaN(data4)) {
                          validate16.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                          return false;
                        } else {
                          if (data4 < 1 || isNaN(data4)) {
                            validate16.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                            return false;
                          }
                        }
                      }
                    }
                    var valid0 = _errs14 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.contracts !== undefined) {
                      const _errs17 = errors;
                      if (!validate17(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                        vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs17 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.status !== undefined) {
                        let data6 = data.status;
                        const _errs18 = errors;
                        if (!(data6 === "proposed" || data6 === "admitted" || data6 === "active" || data6 === "verified" || data6 === "accepted" || data6 === "cancelled")) {
                          validate16.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema21.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
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
        }
      }
    } else {
      validate16.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate16.errors = vErrors;
  return errors === 0;
}
var schema38 = { type: "object", properties: { projectId: { $ref: "#/$defs/Id" }, workId: { $ref: "#/$defs/Id" }, taskId: { $ref: "#/$defs/Id" }, title: { $ref: "#/$defs/Id" }, parentTaskId: { $ref: "#/$defs/Id" }, assigneeActorId: { $ref: "#/$defs/Id" }, revision: { $ref: "#/$defs/Revision" }, contractOverrides: { $ref: "#/$defs/ContractOverrides" }, acceptanceCriteria: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2000 }, resources: { type: "array", items: { $ref: "#/$defs/ResourceClaim" }, maxItems: 2000 }, status: { enum: ["planned", "ready", "running", "blocked", "verified", "accepted", "cancelled"] } }, required: ["projectId", "workId", "taskId", "title", "assigneeActorId", "revision", "contractOverrides", "acceptanceCriteria", "resources", "status"], additionalProperties: false };
function validate27(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      const _errs1 = errors;
      for (const key0 in data) {
        if (!(key0 === "issue" || key0 === "spec" || key0 === "plan")) {
          validate27.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
          return false;
          break;
        }
      }
      if (_errs1 === errors) {
        if (data.issue !== undefined) {
          const _errs2 = errors;
          if (!validate18(data.issue, { instancePath: instancePath + "/issue", parentData: data, parentDataProperty: "issue", rootData })) {
            vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
            errors = vErrors.length;
          }
          var valid0 = _errs2 === errors;
        } else {
          var valid0 = true;
        }
        if (valid0) {
          if (data.spec !== undefined) {
            const _errs3 = errors;
            if (!validate18(data.spec, { instancePath: instancePath + "/spec", parentData: data, parentDataProperty: "spec", rootData })) {
              vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs3 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.plan !== undefined) {
              const _errs4 = errors;
              if (!validate18(data.plan, { instancePath: instancePath + "/plan", parentData: data, parentDataProperty: "plan", rootData })) {
                vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
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
      validate27.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate27.errors = vErrors;
  return errors === 0;
}
var schema48 = { type: "object", properties: { resource: { $ref: "#/$defs/Id" }, mode: { enum: ["read", "write"] } }, required: ["resource", "mode"], additionalProperties: false };
function validate32(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.resource === undefined && (missing0 = "resource") || data.mode === undefined && (missing0 = "mode")) {
        validate32.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "resource" || key0 === "mode")) {
            validate32.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.resource !== undefined) {
            let data0 = data.resource;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate32.errors = [{ instancePath: instancePath + "/resource", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate32.errors = [{ instancePath: instancePath + "/resource", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate32.errors = [{ instancePath: instancePath + "/resource", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate32.errors = [{ instancePath: instancePath + "/resource", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.mode !== undefined) {
              let data1 = data.mode;
              const _errs5 = errors;
              if (!(data1 === "read" || data1 === "write")) {
                validate32.errors = [{ instancePath: instancePath + "/mode", schemaPath: "#/properties/mode/enum", keyword: "enum", params: { allowedValues: schema48.properties.mode.enum }, message: "must be equal to one of the allowed values" }];
                return false;
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate32.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate32.errors = vErrors;
  return errors === 0;
}
function validate26(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.taskId === undefined && (missing0 = "taskId") || data.title === undefined && (missing0 = "title") || data.assigneeActorId === undefined && (missing0 = "assigneeActorId") || data.revision === undefined && (missing0 = "revision") || data.contractOverrides === undefined && (missing0 = "contractOverrides") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.resources === undefined && (missing0 = "resources") || data.status === undefined && (missing0 = "status")) {
        validate26.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema38.properties, key0)) {
            validate26.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            let data0 = data.projectId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate26.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate26.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate26.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate26.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              let data1 = data.workId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate26.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate26.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate26.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate26.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskId !== undefined) {
                let data2 = data.taskId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate26.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate26.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate26.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate26.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs8 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.title !== undefined) {
                  let data3 = data.title;
                  const _errs11 = errors;
                  const _errs12 = errors;
                  if (errors === _errs12) {
                    if (typeof data3 === "string") {
                      if (func3(data3) > 240) {
                        validate26.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                        return false;
                      } else {
                        if (func3(data3) < 1) {
                          validate26.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        } else {
                          if (!pattern0.test(data3)) {
                            validate26.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                            return false;
                          }
                        }
                      }
                    } else {
                      validate26.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs11 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.parentTaskId !== undefined) {
                    let data4 = data.parentTaskId;
                    const _errs14 = errors;
                    const _errs15 = errors;
                    if (errors === _errs15) {
                      if (typeof data4 === "string") {
                        if (func3(data4) > 240) {
                          validate26.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                          return false;
                        } else {
                          if (func3(data4) < 1) {
                            validate26.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                            return false;
                          } else {
                            if (!pattern0.test(data4)) {
                              validate26.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                              return false;
                            }
                          }
                        }
                      } else {
                        validate26.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid0 = _errs14 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.assigneeActorId !== undefined) {
                      let data5 = data.assigneeActorId;
                      const _errs17 = errors;
                      const _errs18 = errors;
                      if (errors === _errs18) {
                        if (typeof data5 === "string") {
                          if (func3(data5) > 240) {
                            validate26.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                            return false;
                          } else {
                            if (func3(data5) < 1) {
                              validate26.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            } else {
                              if (!pattern0.test(data5)) {
                                validate26.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                return false;
                              }
                            }
                          }
                        } else {
                          validate26.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid0 = _errs17 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.revision !== undefined) {
                        let data6 = data.revision;
                        const _errs20 = errors;
                        const _errs21 = errors;
                        if (!(typeof data6 == "number" && (!(data6 % 1) && !isNaN(data6)) && isFinite(data6))) {
                          validate26.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                          return false;
                        }
                        if (errors === _errs21) {
                          if (typeof data6 == "number" && isFinite(data6)) {
                            if (data6 > 9007199254740991 || isNaN(data6)) {
                              validate26.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                              return false;
                            } else {
                              if (data6 < 1 || isNaN(data6)) {
                                validate26.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                return false;
                              }
                            }
                          }
                        }
                        var valid0 = _errs20 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.contractOverrides !== undefined) {
                          const _errs23 = errors;
                          if (!validate27(data.contractOverrides, { instancePath: instancePath + "/contractOverrides", parentData: data, parentDataProperty: "contractOverrides", rootData })) {
                            vErrors = vErrors === null ? validate27.errors : vErrors.concat(validate27.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs23 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.acceptanceCriteria !== undefined) {
                            let data8 = data.acceptanceCriteria;
                            const _errs24 = errors;
                            if (errors === _errs24) {
                              if (Array.isArray(data8)) {
                                if (data8.length > 2000) {
                                  validate26.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid8 = true;
                                  const len0 = data8.length;
                                  for (let i0 = 0;i0 < len0; i0++) {
                                    let data9 = data8[i0];
                                    const _errs26 = errors;
                                    const _errs27 = errors;
                                    if (errors === _errs27) {
                                      if (typeof data9 === "string") {
                                        if (func3(data9) > 240) {
                                          validate26.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                          return false;
                                        } else {
                                          if (func3(data9) < 1) {
                                            validate26.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                            return false;
                                          } else {
                                            if (!pattern0.test(data9)) {
                                              validate26.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                              return false;
                                            }
                                          }
                                        }
                                      } else {
                                        validate26.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid8 = _errs26 === errors;
                                    if (!valid8) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate26.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs24 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.resources !== undefined) {
                              let data10 = data.resources;
                              const _errs29 = errors;
                              if (errors === _errs29) {
                                if (Array.isArray(data10)) {
                                  if (data10.length > 2000) {
                                    validate26.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid10 = true;
                                    const len1 = data10.length;
                                    for (let i1 = 0;i1 < len1; i1++) {
                                      const _errs31 = errors;
                                      if (!validate32(data10[i1], { instancePath: instancePath + "/resources/" + i1, parentData: data10, parentDataProperty: i1, rootData })) {
                                        vErrors = vErrors === null ? validate32.errors : vErrors.concat(validate32.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid10 = _errs31 === errors;
                                      if (!valid10) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate26.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs29 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.status !== undefined) {
                                let data12 = data.status;
                                const _errs32 = errors;
                                if (!(data12 === "planned" || data12 === "ready" || data12 === "running" || data12 === "blocked" || data12 === "verified" || data12 === "accepted" || data12 === "cancelled")) {
                                  validate26.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema38.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid0 = _errs32 === errors;
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
    } else {
      validate26.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate26.errors = vErrors;
  return errors === 0;
}
function validate37(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.owner === undefined && (missing0 = "owner") || data.uri === undefined && (missing0 = "uri") || data.revision === undefined && (missing0 = "revision") || data.digest === undefined && (missing0 = "digest")) {
        validate37.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "owner" || key0 === "uri" || key0 === "revision" || key0 === "digest")) {
            validate37.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.owner !== undefined) {
            let data0 = data.owner;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate37.errors = [{ instancePath: instancePath + "/owner", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate37.errors = [{ instancePath: instancePath + "/owner", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate37.errors = [{ instancePath: instancePath + "/owner", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate37.errors = [{ instancePath: instancePath + "/owner", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.uri !== undefined) {
              let data1 = data.uri;
              const _errs5 = errors;
              if (errors === _errs5) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 4096) {
                    validate37.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate37.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    }
                  }
                } else {
                  validate37.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.revision !== undefined) {
                let data2 = data.revision;
                const _errs7 = errors;
                const _errs8 = errors;
                if (errors === _errs8) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate37.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate37.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate37.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate37.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs7 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.digest !== undefined) {
                  let data3 = data.digest;
                  const _errs10 = errors;
                  const _errs11 = errors;
                  if (errors === _errs11) {
                    if (typeof data3 === "string") {
                      if (!pattern16.test(data3)) {
                        validate37.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                        return false;
                      }
                    } else {
                      validate37.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
    } else {
      validate37.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate37.errors = vErrors;
  return errors === 0;
}
var wrapper0 = { validate: validate40 };
function validate40(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
    validate40.errors = vErrors;
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
  validate40.errors = vErrors;
  return errors === 0;
}
function validate39(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      const _errs1 = errors;
      for (const key0 in data) {
        if (!(key0 === "parentIssue")) {
          const _errs2 = errors;
          if (!validate40(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
            vErrors = vErrors === null ? validate40.errors : vErrors.concat(validate40.errors);
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
          if (!validate18(data.parentIssue, { instancePath: instancePath + "/parentIssue", parentData: data, parentDataProperty: "parentIssue", rootData })) {
            vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
            errors = vErrors.length;
          }
        }
      }
    } else {
      validate39.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate39.errors = vErrors;
  return errors === 0;
}
function validate48(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id")) {
        validate48.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "covers")) {
            const _errs2 = errors;
            if (!validate40(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
              vErrors = vErrors === null ? validate40.errors : vErrors.concat(validate40.errors);
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
            let data1 = data.id;
            const _errs3 = errors;
            const _errs4 = errors;
            if (errors === _errs4) {
              if (typeof data1 === "string") {
                if (func3(data1) > 240) {
                  validate48.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data1) < 1) {
                    validate48.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data1)) {
                      validate48.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate48.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid1 = _errs3 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.covers !== undefined) {
              let data2 = data.covers;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (Array.isArray(data2)) {
                  if (data2.length > 2000) {
                    validate48.errors = [{ instancePath: instancePath + "/covers", schemaPath: "#/properties/covers/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                    return false;
                  } else {
                    var valid3 = true;
                    const len0 = data2.length;
                    for (let i0 = 0;i0 < len0; i0++) {
                      let data3 = data2[i0];
                      const _errs8 = errors;
                      const _errs9 = errors;
                      if (errors === _errs9) {
                        if (typeof data3 === "string") {
                          if (func3(data3) > 240) {
                            validate48.errors = [{ instancePath: instancePath + "/covers/" + i0, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                            return false;
                          } else {
                            if (func3(data3) < 1) {
                              validate48.errors = [{ instancePath: instancePath + "/covers/" + i0, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            } else {
                              if (!pattern0.test(data3)) {
                                validate48.errors = [{ instancePath: instancePath + "/covers/" + i0, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                return false;
                              }
                            }
                          }
                        } else {
                          validate48.errors = [{ instancePath: instancePath + "/covers/" + i0, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid3 = _errs8 === errors;
                      if (!valid3) {
                        break;
                      }
                    }
                  }
                } else {
                  validate48.errors = [{ instancePath: instancePath + "/covers", schemaPath: "#/properties/covers/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid1 = _errs6 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      }
    } else {
      validate48.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate48.errors = vErrors;
  return errors === 0;
}
function validate46(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria")) {
        validate46.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "acceptanceCriteria" || key0 === "refinesSpec")) {
            const _errs2 = errors;
            if (!validate40(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
              vErrors = vErrors === null ? validate40.errors : vErrors.concat(validate40.errors);
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
                  validate46.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                  return false;
                } else {
                  var valid2 = true;
                  const len0 = data1.length;
                  for (let i0 = 0;i0 < len0; i0++) {
                    const _errs5 = errors;
                    if (!validate48(data1[i0], { instancePath: instancePath + "/acceptanceCriteria/" + i0, parentData: data1, parentDataProperty: i0, rootData })) {
                      vErrors = vErrors === null ? validate48.errors : vErrors.concat(validate48.errors);
                      errors = vErrors.length;
                    }
                    var valid2 = _errs5 === errors;
                    if (!valid2) {
                      break;
                    }
                  }
                }
              } else {
                validate46.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
              if (!validate18(data.refinesSpec, { instancePath: instancePath + "/refinesSpec", parentData: data, parentDataProperty: "refinesSpec", rootData })) {
                vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
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
      validate46.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate46.errors = vErrors;
  return errors === 0;
}
function validate55(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.appliesToSpec === undefined && (missing0 = "appliesToSpec")) {
        validate55.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "appliesToSpec" || key0 === "methodRef")) {
            const _errs2 = errors;
            if (!validate40(data[key0], { instancePath: instancePath + "/" + key0.replace(/~/g, "~0").replace(/\//g, "~1"), parentData: data, parentDataProperty: key0, rootData })) {
              vErrors = vErrors === null ? validate40.errors : vErrors.concat(validate40.errors);
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
            if (!validate18(data.appliesToSpec, { instancePath: instancePath + "/appliesToSpec", parentData: data, parentDataProperty: "appliesToSpec", rootData })) {
              vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
              errors = vErrors.length;
            }
            var valid1 = _errs3 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.methodRef !== undefined) {
              const _errs4 = errors;
              if (!validate18(data.methodRef, { instancePath: instancePath + "/methodRef", parentData: data, parentDataProperty: "methodRef", rootData })) {
                vErrors = vErrors === null ? validate18.errors : vErrors.concat(validate18.errors);
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
      validate55.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate55.errors = vErrors;
  return errors === 0;
}
function validate35(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
            let data0 = data.id;
            const _errs4 = errors;
            const _errs5 = errors;
            if (errors === _errs5) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  const err2 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                  if (vErrors === null) {
                    vErrors = [err2];
                  } else {
                    vErrors.push(err2);
                  }
                  errors++;
                } else {
                  if (func3(data0) < 1) {
                    const err3 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                    if (vErrors === null) {
                      vErrors = [err3];
                    } else {
                      vErrors.push(err3);
                    }
                    errors++;
                  } else {
                    if (!pattern0.test(data0)) {
                      const err4 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                      if (vErrors === null) {
                        vErrors = [err4];
                      } else {
                        vErrors.push(err4);
                      }
                      errors++;
                    }
                  }
                }
              } else {
                const err5 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                if (vErrors === null) {
                  vErrors = [err5];
                } else {
                  vErrors.push(err5);
                }
                errors++;
              }
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.owner !== undefined) {
              const _errs7 = errors;
              if (!validate19(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate19.errors : vErrors.concat(validate19.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs7 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.revision !== undefined) {
                let data2 = data.revision;
                const _errs8 = errors;
                const _errs9 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  const err6 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
                  if (vErrors === null) {
                    vErrors = [err6];
                  } else {
                    vErrors.push(err6);
                  }
                  errors++;
                }
                if (errors === _errs9) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      const err7 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
                      if (vErrors === null) {
                        vErrors = [err7];
                      } else {
                        vErrors.push(err7);
                      }
                      errors++;
                    } else {
                      if (data2 < 1 || isNaN(data2)) {
                        const err8 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" };
                        if (vErrors === null) {
                          vErrors = [err8];
                        } else {
                          vErrors.push(err8);
                        }
                        errors++;
                      }
                    }
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
              if (valid1) {
                if (data.kind !== undefined) {
                  const _errs11 = errors;
                  if (data.kind !== "issue") {
                    const err9 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/0/properties/kind/const", keyword: "const", params: { allowedValue: "issue" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err9];
                    } else {
                      vErrors.push(err9);
                    }
                    errors++;
                  }
                  var valid1 = _errs11 === errors;
                } else {
                  var valid1 = true;
                }
                if (valid1) {
                  if (data.digest !== undefined) {
                    let data4 = data.digest;
                    const _errs12 = errors;
                    const _errs13 = errors;
                    if (errors === _errs13) {
                      if (typeof data4 === "string") {
                        if (!pattern16.test(data4)) {
                          const err10 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err10];
                          } else {
                            vErrors.push(err10);
                          }
                          errors++;
                        }
                      } else {
                        const err11 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                        if (vErrors === null) {
                          vErrors = [err11];
                        } else {
                          vErrors.push(err11);
                        }
                        errors++;
                      }
                    }
                    var valid1 = _errs12 === errors;
                  } else {
                    var valid1 = true;
                  }
                  if (valid1) {
                    if (data.sourceRefs !== undefined) {
                      let data5 = data.sourceRefs;
                      const _errs15 = errors;
                      if (errors === _errs15) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2000) {
                            const err12 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err12];
                            } else {
                              vErrors.push(err12);
                            }
                            errors++;
                          } else {
                            var valid5 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs17 = errors;
                              if (!validate37(data5[i0], { instancePath: instancePath + "/sourceRefs/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
                                errors = vErrors.length;
                              }
                              var valid5 = _errs17 === errors;
                              if (!valid5) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err13 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err13];
                          } else {
                            vErrors.push(err13);
                          }
                          errors++;
                        }
                      }
                      var valid1 = _errs15 === errors;
                    } else {
                      var valid1 = true;
                    }
                    if (valid1) {
                      if (data.body !== undefined) {
                        const _errs18 = errors;
                        if (!validate39(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                          errors = vErrors.length;
                        }
                        var valid1 = _errs18 === errors;
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
      const err14 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err14];
      } else {
        vErrors.push(err14);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs19 = errors;
  if (errors === _errs19) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.id === undefined && (missing1 = "id") || data.owner === undefined && (missing1 = "owner") || data.revision === undefined && (missing1 = "revision") || data.kind === undefined && (missing1 = "kind") || data.digest === undefined && (missing1 = "digest") || data.sourceRefs === undefined && (missing1 = "sourceRefs") || data.body === undefined && (missing1 = "body")) {
        const err15 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err15];
        } else {
          vErrors.push(err15);
        }
        errors++;
      } else {
        const _errs21 = errors;
        for (const key1 in data) {
          if (!(key1 === "id" || key1 === "owner" || key1 === "revision" || key1 === "kind" || key1 === "digest" || key1 === "sourceRefs" || key1 === "body")) {
            const err16 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err16];
            } else {
              vErrors.push(err16);
            }
            errors++;
            break;
          }
        }
        if (_errs21 === errors) {
          if (data.id !== undefined) {
            let data8 = data.id;
            const _errs22 = errors;
            const _errs23 = errors;
            if (errors === _errs23) {
              if (typeof data8 === "string") {
                if (func3(data8) > 240) {
                  const err17 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                  if (vErrors === null) {
                    vErrors = [err17];
                  } else {
                    vErrors.push(err17);
                  }
                  errors++;
                } else {
                  if (func3(data8) < 1) {
                    const err18 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                    if (vErrors === null) {
                      vErrors = [err18];
                    } else {
                      vErrors.push(err18);
                    }
                    errors++;
                  } else {
                    if (!pattern0.test(data8)) {
                      const err19 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                      if (vErrors === null) {
                        vErrors = [err19];
                      } else {
                        vErrors.push(err19);
                      }
                      errors++;
                    }
                  }
                }
              } else {
                const err20 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                if (vErrors === null) {
                  vErrors = [err20];
                } else {
                  vErrors.push(err20);
                }
                errors++;
              }
            }
            var valid6 = _errs22 === errors;
          } else {
            var valid6 = true;
          }
          if (valid6) {
            if (data.owner !== undefined) {
              const _errs25 = errors;
              if (!validate19(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate19.errors : vErrors.concat(validate19.errors);
                errors = vErrors.length;
              }
              var valid6 = _errs25 === errors;
            } else {
              var valid6 = true;
            }
            if (valid6) {
              if (data.revision !== undefined) {
                let data10 = data.revision;
                const _errs26 = errors;
                const _errs27 = errors;
                if (!(typeof data10 == "number" && (!(data10 % 1) && !isNaN(data10)) && isFinite(data10))) {
                  const err21 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
                  if (vErrors === null) {
                    vErrors = [err21];
                  } else {
                    vErrors.push(err21);
                  }
                  errors++;
                }
                if (errors === _errs27) {
                  if (typeof data10 == "number" && isFinite(data10)) {
                    if (data10 > 9007199254740991 || isNaN(data10)) {
                      const err22 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
                      if (vErrors === null) {
                        vErrors = [err22];
                      } else {
                        vErrors.push(err22);
                      }
                      errors++;
                    } else {
                      if (data10 < 1 || isNaN(data10)) {
                        const err23 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" };
                        if (vErrors === null) {
                          vErrors = [err23];
                        } else {
                          vErrors.push(err23);
                        }
                        errors++;
                      }
                    }
                  }
                }
                var valid6 = _errs26 === errors;
              } else {
                var valid6 = true;
              }
              if (valid6) {
                if (data.kind !== undefined) {
                  const _errs29 = errors;
                  if (data.kind !== "spec") {
                    const err24 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "spec" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err24];
                    } else {
                      vErrors.push(err24);
                    }
                    errors++;
                  }
                  var valid6 = _errs29 === errors;
                } else {
                  var valid6 = true;
                }
                if (valid6) {
                  if (data.digest !== undefined) {
                    let data12 = data.digest;
                    const _errs30 = errors;
                    const _errs31 = errors;
                    if (errors === _errs31) {
                      if (typeof data12 === "string") {
                        if (!pattern16.test(data12)) {
                          const err25 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err25];
                          } else {
                            vErrors.push(err25);
                          }
                          errors++;
                        }
                      } else {
                        const err26 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                        if (vErrors === null) {
                          vErrors = [err26];
                        } else {
                          vErrors.push(err26);
                        }
                        errors++;
                      }
                    }
                    var valid6 = _errs30 === errors;
                  } else {
                    var valid6 = true;
                  }
                  if (valid6) {
                    if (data.sourceRefs !== undefined) {
                      let data13 = data.sourceRefs;
                      const _errs33 = errors;
                      if (errors === _errs33) {
                        if (Array.isArray(data13)) {
                          if (data13.length > 2000) {
                            const err27 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err27];
                            } else {
                              vErrors.push(err27);
                            }
                            errors++;
                          } else {
                            var valid10 = true;
                            const len1 = data13.length;
                            for (let i1 = 0;i1 < len1; i1++) {
                              const _errs35 = errors;
                              if (!validate37(data13[i1], { instancePath: instancePath + "/sourceRefs/" + i1, parentData: data13, parentDataProperty: i1, rootData })) {
                                vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
                                errors = vErrors.length;
                              }
                              var valid10 = _errs35 === errors;
                              if (!valid10) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err28 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err28];
                          } else {
                            vErrors.push(err28);
                          }
                          errors++;
                        }
                      }
                      var valid6 = _errs33 === errors;
                    } else {
                      var valid6 = true;
                    }
                    if (valid6) {
                      if (data.body !== undefined) {
                        const _errs36 = errors;
                        if (!validate46(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
                          errors = vErrors.length;
                        }
                        var valid6 = _errs36 === errors;
                      } else {
                        var valid6 = true;
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
      const err29 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err29];
      } else {
        vErrors.push(err29);
      }
      errors++;
    }
  }
  var _valid0 = _errs19 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
    const _errs37 = errors;
    if (errors === _errs37) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.id === undefined && (missing2 = "id") || data.owner === undefined && (missing2 = "owner") || data.revision === undefined && (missing2 = "revision") || data.kind === undefined && (missing2 = "kind") || data.digest === undefined && (missing2 = "digest") || data.sourceRefs === undefined && (missing2 = "sourceRefs") || data.body === undefined && (missing2 = "body")) {
          const err30 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err30];
          } else {
            vErrors.push(err30);
          }
          errors++;
        } else {
          const _errs39 = errors;
          for (const key2 in data) {
            if (!(key2 === "id" || key2 === "owner" || key2 === "revision" || key2 === "kind" || key2 === "digest" || key2 === "sourceRefs" || key2 === "body")) {
              const err31 = { instancePath, schemaPath: "#/oneOf/2/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" };
              if (vErrors === null) {
                vErrors = [err31];
              } else {
                vErrors.push(err31);
              }
              errors++;
              break;
            }
          }
          if (_errs39 === errors) {
            if (data.id !== undefined) {
              let data16 = data.id;
              const _errs40 = errors;
              const _errs41 = errors;
              if (errors === _errs41) {
                if (typeof data16 === "string") {
                  if (func3(data16) > 240) {
                    const err32 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                    if (vErrors === null) {
                      vErrors = [err32];
                    } else {
                      vErrors.push(err32);
                    }
                    errors++;
                  } else {
                    if (func3(data16) < 1) {
                      const err33 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                      if (vErrors === null) {
                        vErrors = [err33];
                      } else {
                        vErrors.push(err33);
                      }
                      errors++;
                    } else {
                      if (!pattern0.test(data16)) {
                        const err34 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                        if (vErrors === null) {
                          vErrors = [err34];
                        } else {
                          vErrors.push(err34);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err35 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err35];
                  } else {
                    vErrors.push(err35);
                  }
                  errors++;
                }
              }
              var valid11 = _errs40 === errors;
            } else {
              var valid11 = true;
            }
            if (valid11) {
              if (data.owner !== undefined) {
                const _errs43 = errors;
                if (!validate19(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                  vErrors = vErrors === null ? validate19.errors : vErrors.concat(validate19.errors);
                  errors = vErrors.length;
                }
                var valid11 = _errs43 === errors;
              } else {
                var valid11 = true;
              }
              if (valid11) {
                if (data.revision !== undefined) {
                  let data18 = data.revision;
                  const _errs44 = errors;
                  const _errs45 = errors;
                  if (!(typeof data18 == "number" && (!(data18 % 1) && !isNaN(data18)) && isFinite(data18))) {
                    const err36 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
                    if (vErrors === null) {
                      vErrors = [err36];
                    } else {
                      vErrors.push(err36);
                    }
                    errors++;
                  }
                  if (errors === _errs45) {
                    if (typeof data18 == "number" && isFinite(data18)) {
                      if (data18 > 9007199254740991 || isNaN(data18)) {
                        const err37 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
                        if (vErrors === null) {
                          vErrors = [err37];
                        } else {
                          vErrors.push(err37);
                        }
                        errors++;
                      } else {
                        if (data18 < 1 || isNaN(data18)) {
                          const err38 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" };
                          if (vErrors === null) {
                            vErrors = [err38];
                          } else {
                            vErrors.push(err38);
                          }
                          errors++;
                        }
                      }
                    }
                  }
                  var valid11 = _errs44 === errors;
                } else {
                  var valid11 = true;
                }
                if (valid11) {
                  if (data.kind !== undefined) {
                    const _errs47 = errors;
                    if (data.kind !== "plan") {
                      const err39 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/2/properties/kind/const", keyword: "const", params: { allowedValue: "plan" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err39];
                      } else {
                        vErrors.push(err39);
                      }
                      errors++;
                    }
                    var valid11 = _errs47 === errors;
                  } else {
                    var valid11 = true;
                  }
                  if (valid11) {
                    if (data.digest !== undefined) {
                      let data20 = data.digest;
                      const _errs48 = errors;
                      const _errs49 = errors;
                      if (errors === _errs49) {
                        if (typeof data20 === "string") {
                          if (!pattern16.test(data20)) {
                            const err40 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' };
                            if (vErrors === null) {
                              vErrors = [err40];
                            } else {
                              vErrors.push(err40);
                            }
                            errors++;
                          }
                        } else {
                          const err41 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                          if (vErrors === null) {
                            vErrors = [err41];
                          } else {
                            vErrors.push(err41);
                          }
                          errors++;
                        }
                      }
                      var valid11 = _errs48 === errors;
                    } else {
                      var valid11 = true;
                    }
                    if (valid11) {
                      if (data.sourceRefs !== undefined) {
                        let data21 = data.sourceRefs;
                        const _errs51 = errors;
                        if (errors === _errs51) {
                          if (Array.isArray(data21)) {
                            if (data21.length > 2000) {
                              const err42 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                              if (vErrors === null) {
                                vErrors = [err42];
                              } else {
                                vErrors.push(err42);
                              }
                              errors++;
                            } else {
                              var valid15 = true;
                              const len2 = data21.length;
                              for (let i2 = 0;i2 < len2; i2++) {
                                const _errs53 = errors;
                                if (!validate37(data21[i2], { instancePath: instancePath + "/sourceRefs/" + i2, parentData: data21, parentDataProperty: i2, rootData })) {
                                  vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
                                  errors = vErrors.length;
                                }
                                var valid15 = _errs53 === errors;
                                if (!valid15) {
                                  break;
                                }
                              }
                            }
                          } else {
                            const err43 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                            if (vErrors === null) {
                              vErrors = [err43];
                            } else {
                              vErrors.push(err43);
                            }
                            errors++;
                          }
                        }
                        var valid11 = _errs51 === errors;
                      } else {
                        var valid11 = true;
                      }
                      if (valid11) {
                        if (data.body !== undefined) {
                          const _errs54 = errors;
                          if (!validate55(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                            vErrors = vErrors === null ? validate55.errors : vErrors.concat(validate55.errors);
                            errors = vErrors.length;
                          }
                          var valid11 = _errs54 === errors;
                        } else {
                          var valid11 = true;
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
        const err44 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err44];
        } else {
          vErrors.push(err44);
        }
        errors++;
      }
    }
    var _valid0 = _errs37 === errors;
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
    const err45 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err45];
    } else {
      vErrors.push(err45);
    }
    errors++;
    validate35.errors = vErrors;
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
  validate35.errors = vErrors;
  return errors === 0;
}
var schema71 = { type: "object", properties: { producer: { $ref: "#/$defs/TaskRef" }, consumer: { $ref: "#/$defs/TaskRef" }, required: { enum: ["verified", "accepted"] }, artifact: { $ref: "#/$defs/ArtifactRef" } }, required: ["producer", "consumer", "required"], additionalProperties: false };
function validate62(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.taskId === undefined && (missing0 = "taskId")) {
        validate62.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId" || key0 === "taskId")) {
            validate62.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            let data0 = data.projectId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate62.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate62.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate62.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate62.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              let data1 = data.workId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate62.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate62.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate62.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate62.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskId !== undefined) {
                let data2 = data.taskId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate62.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate62.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate62.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate62.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
    } else {
      validate62.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate62.errors = vErrors;
  return errors === 0;
}
function validate65(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.revision === undefined && (missing0 = "revision") || data.digest === undefined && (missing0 = "digest")) {
        validate65.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "revision" || key0 === "digest")) {
            validate65.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== undefined) {
            let data0 = data.id;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate65.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate65.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate65.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate65.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.revision !== undefined) {
              let data1 = data.revision;
              const _errs5 = errors;
              const _errs6 = errors;
              if (!(typeof data1 == "number" && (!(data1 % 1) && !isNaN(data1)) && isFinite(data1))) {
                validate65.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                return false;
              }
              if (errors === _errs6) {
                if (typeof data1 == "number" && isFinite(data1)) {
                  if (data1 > 9007199254740991 || isNaN(data1)) {
                    validate65.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                    return false;
                  } else {
                    if (data1 < 1 || isNaN(data1)) {
                      validate65.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
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
              if (data.digest !== undefined) {
                let data2 = data.digest;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (!pattern16.test(data2)) {
                      validate65.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                      return false;
                    }
                  } else {
                    validate65.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
    } else {
      validate65.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate65.errors = vErrors;
  return errors === 0;
}
function validate61(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.producer === undefined && (missing0 = "producer") || data.consumer === undefined && (missing0 = "consumer") || data.required === undefined && (missing0 = "required")) {
        validate61.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "producer" || key0 === "consumer" || key0 === "required" || key0 === "artifact")) {
            validate61.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.producer !== undefined) {
            const _errs2 = errors;
            if (!validate62(data.producer, { instancePath: instancePath + "/producer", parentData: data, parentDataProperty: "producer", rootData })) {
              vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.consumer !== undefined) {
              const _errs3 = errors;
              if (!validate62(data.consumer, { instancePath: instancePath + "/consumer", parentData: data, parentDataProperty: "consumer", rootData })) {
                vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
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
                  validate61.errors = [{ instancePath: instancePath + "/required", schemaPath: "#/properties/required/enum", keyword: "enum", params: { allowedValues: schema71.properties.required.enum }, message: "must be equal to one of the allowed values" }];
                  return false;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.artifact !== undefined) {
                  const _errs5 = errors;
                  if (!validate65(data.artifact, { instancePath: instancePath + "/artifact", parentData: data, parentDataProperty: "artifact", rootData })) {
                    vErrors = vErrors === null ? validate65.errors : vErrors.concat(validate65.errors);
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
      validate61.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate61.errors = vErrors;
  return errors === 0;
}
function validate69(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
              let data1 = data.userId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    const err3 = { instancePath: instancePath + "/userId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                    if (vErrors === null) {
                      vErrors = [err3];
                    } else {
                      vErrors.push(err3);
                    }
                    errors++;
                  } else {
                    if (func3(data1) < 1) {
                      const err4 = { instancePath: instancePath + "/userId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                      if (vErrors === null) {
                        vErrors = [err4];
                      } else {
                        vErrors.push(err4);
                      }
                      errors++;
                    } else {
                      if (!pattern0.test(data1)) {
                        const err5 = { instancePath: instancePath + "/userId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                        if (vErrors === null) {
                          vErrors = [err5];
                        } else {
                          vErrors.push(err5);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err6 = { instancePath: instancePath + "/userId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err6];
                  } else {
                    vErrors.push(err6);
                  }
                  errors++;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      }
    } else {
      const err7 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err7];
      } else {
        vErrors.push(err7);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs8 = errors;
  if (errors === _errs8) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.kind === undefined && (missing1 = "kind") || data.domainId === undefined && (missing1 = "domainId")) {
        const err8 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err8];
        } else {
          vErrors.push(err8);
        }
        errors++;
      } else {
        const _errs10 = errors;
        for (const key1 in data) {
          if (!(key1 === "kind" || key1 === "domainId")) {
            const err9 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err9];
            } else {
              vErrors.push(err9);
            }
            errors++;
            break;
          }
        }
        if (_errs10 === errors) {
          if (data.kind !== undefined) {
            const _errs11 = errors;
            if (data.kind !== "domain") {
              const err10 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "domain" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err10];
              } else {
                vErrors.push(err10);
              }
              errors++;
            }
            var valid3 = _errs11 === errors;
          } else {
            var valid3 = true;
          }
          if (valid3) {
            if (data.domainId !== undefined) {
              let data3 = data.domainId;
              const _errs12 = errors;
              const _errs13 = errors;
              if (errors === _errs13) {
                if (typeof data3 === "string") {
                  if (func3(data3) > 240) {
                    const err11 = { instancePath: instancePath + "/domainId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                    if (vErrors === null) {
                      vErrors = [err11];
                    } else {
                      vErrors.push(err11);
                    }
                    errors++;
                  } else {
                    if (func3(data3) < 1) {
                      const err12 = { instancePath: instancePath + "/domainId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                      if (vErrors === null) {
                        vErrors = [err12];
                      } else {
                        vErrors.push(err12);
                      }
                      errors++;
                    } else {
                      if (!pattern0.test(data3)) {
                        const err13 = { instancePath: instancePath + "/domainId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                        if (vErrors === null) {
                          vErrors = [err13];
                        } else {
                          vErrors.push(err13);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err14 = { instancePath: instancePath + "/domainId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err14];
                  } else {
                    vErrors.push(err14);
                  }
                  errors++;
                }
              }
              var valid3 = _errs12 === errors;
            } else {
              var valid3 = true;
            }
          }
        }
      }
    } else {
      const err15 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err15];
      } else {
        vErrors.push(err15);
      }
      errors++;
    }
  }
  var _valid0 = _errs8 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
    const _errs15 = errors;
    if (errors === _errs15) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.kind === undefined && (missing2 = "kind") || data.projectId === undefined && (missing2 = "projectId")) {
          const err16 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err16];
          } else {
            vErrors.push(err16);
          }
          errors++;
        } else {
          const _errs17 = errors;
          for (const key2 in data) {
            if (!(key2 === "kind" || key2 === "projectId")) {
              const err17 = { instancePath, schemaPath: "#/oneOf/2/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" };
              if (vErrors === null) {
                vErrors = [err17];
              } else {
                vErrors.push(err17);
              }
              errors++;
              break;
            }
          }
          if (_errs17 === errors) {
            if (data.kind !== undefined) {
              const _errs18 = errors;
              if (data.kind !== "project") {
                const err18 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/2/properties/kind/const", keyword: "const", params: { allowedValue: "project" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err18];
                } else {
                  vErrors.push(err18);
                }
                errors++;
              }
              var valid5 = _errs18 === errors;
            } else {
              var valid5 = true;
            }
            if (valid5) {
              if (data.projectId !== undefined) {
                let data5 = data.projectId;
                const _errs19 = errors;
                const _errs20 = errors;
                if (errors === _errs20) {
                  if (typeof data5 === "string") {
                    if (func3(data5) > 240) {
                      const err19 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                      if (vErrors === null) {
                        vErrors = [err19];
                      } else {
                        vErrors.push(err19);
                      }
                      errors++;
                    } else {
                      if (func3(data5) < 1) {
                        const err20 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                        if (vErrors === null) {
                          vErrors = [err20];
                        } else {
                          vErrors.push(err20);
                        }
                        errors++;
                      } else {
                        if (!pattern0.test(data5)) {
                          const err21 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err21];
                          } else {
                            vErrors.push(err21);
                          }
                          errors++;
                        }
                      }
                    }
                  } else {
                    const err22 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                    if (vErrors === null) {
                      vErrors = [err22];
                    } else {
                      vErrors.push(err22);
                    }
                    errors++;
                  }
                }
                var valid5 = _errs19 === errors;
              } else {
                var valid5 = true;
              }
            }
          }
        }
      } else {
        const err23 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err23];
        } else {
          vErrors.push(err23);
        }
        errors++;
      }
    }
    var _valid0 = _errs15 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
      }
      const _errs22 = errors;
      if (errors === _errs22) {
        if (data && typeof data == "object" && !Array.isArray(data)) {
          let missing3;
          if (data.kind === undefined && (missing3 = "kind") || data.projectId === undefined && (missing3 = "projectId") || data.workId === undefined && (missing3 = "workId")) {
            const err24 = { instancePath, schemaPath: "#/oneOf/3/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" };
            if (vErrors === null) {
              vErrors = [err24];
            } else {
              vErrors.push(err24);
            }
            errors++;
          } else {
            const _errs24 = errors;
            for (const key3 in data) {
              if (!(key3 === "kind" || key3 === "projectId" || key3 === "workId")) {
                const err25 = { instancePath, schemaPath: "#/oneOf/3/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key3 }, message: "must NOT have additional properties" };
                if (vErrors === null) {
                  vErrors = [err25];
                } else {
                  vErrors.push(err25);
                }
                errors++;
                break;
              }
            }
            if (_errs24 === errors) {
              if (data.kind !== undefined) {
                const _errs25 = errors;
                if (data.kind !== "work") {
                  const err26 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/3/properties/kind/const", keyword: "const", params: { allowedValue: "work" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err26];
                  } else {
                    vErrors.push(err26);
                  }
                  errors++;
                }
                var valid7 = _errs25 === errors;
              } else {
                var valid7 = true;
              }
              if (valid7) {
                if (data.projectId !== undefined) {
                  let data7 = data.projectId;
                  const _errs26 = errors;
                  const _errs27 = errors;
                  if (errors === _errs27) {
                    if (typeof data7 === "string") {
                      if (func3(data7) > 240) {
                        const err27 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                        if (vErrors === null) {
                          vErrors = [err27];
                        } else {
                          vErrors.push(err27);
                        }
                        errors++;
                      } else {
                        if (func3(data7) < 1) {
                          const err28 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                          if (vErrors === null) {
                            vErrors = [err28];
                          } else {
                            vErrors.push(err28);
                          }
                          errors++;
                        } else {
                          if (!pattern0.test(data7)) {
                            const err29 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                            if (vErrors === null) {
                              vErrors = [err29];
                            } else {
                              vErrors.push(err29);
                            }
                            errors++;
                          }
                        }
                      }
                    } else {
                      const err30 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                      if (vErrors === null) {
                        vErrors = [err30];
                      } else {
                        vErrors.push(err30);
                      }
                      errors++;
                    }
                  }
                  var valid7 = _errs26 === errors;
                } else {
                  var valid7 = true;
                }
                if (valid7) {
                  if (data.workId !== undefined) {
                    let data8 = data.workId;
                    const _errs29 = errors;
                    const _errs30 = errors;
                    if (errors === _errs30) {
                      if (typeof data8 === "string") {
                        if (func3(data8) > 240) {
                          const err31 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                          if (vErrors === null) {
                            vErrors = [err31];
                          } else {
                            vErrors.push(err31);
                          }
                          errors++;
                        } else {
                          if (func3(data8) < 1) {
                            const err32 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                            if (vErrors === null) {
                              vErrors = [err32];
                            } else {
                              vErrors.push(err32);
                            }
                            errors++;
                          } else {
                            if (!pattern0.test(data8)) {
                              const err33 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                              if (vErrors === null) {
                                vErrors = [err33];
                              } else {
                                vErrors.push(err33);
                              }
                              errors++;
                            }
                          }
                        }
                      } else {
                        const err34 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                        if (vErrors === null) {
                          vErrors = [err34];
                        } else {
                          vErrors.push(err34);
                        }
                        errors++;
                      }
                    }
                    var valid7 = _errs29 === errors;
                  } else {
                    var valid7 = true;
                  }
                }
              }
            }
          }
        } else {
          const err35 = { instancePath, schemaPath: "#/oneOf/3/type", keyword: "type", params: { type: "object" }, message: "must be object" };
          if (vErrors === null) {
            vErrors = [err35];
          } else {
            vErrors.push(err35);
          }
          errors++;
        }
      }
      var _valid0 = _errs22 === errors;
      if (_valid0 && valid0) {
        valid0 = false;
        passing0 = [passing0, 3];
      } else {
        if (_valid0) {
          valid0 = true;
          passing0 = 3;
        }
        const _errs32 = errors;
        if (errors === _errs32) {
          if (data && typeof data == "object" && !Array.isArray(data)) {
            let missing4;
            if (data.kind === undefined && (missing4 = "kind") || data.projectId === undefined && (missing4 = "projectId") || data.workId === undefined && (missing4 = "workId") || data.taskId === undefined && (missing4 = "taskId")) {
              const err36 = { instancePath, schemaPath: "#/oneOf/4/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" };
              if (vErrors === null) {
                vErrors = [err36];
              } else {
                vErrors.push(err36);
              }
              errors++;
            } else {
              const _errs34 = errors;
              for (const key4 in data) {
                if (!(key4 === "kind" || key4 === "projectId" || key4 === "workId" || key4 === "taskId")) {
                  const err37 = { instancePath, schemaPath: "#/oneOf/4/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key4 }, message: "must NOT have additional properties" };
                  if (vErrors === null) {
                    vErrors = [err37];
                  } else {
                    vErrors.push(err37);
                  }
                  errors++;
                  break;
                }
              }
              if (_errs34 === errors) {
                if (data.kind !== undefined) {
                  const _errs35 = errors;
                  if (data.kind !== "task") {
                    const err38 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/4/properties/kind/const", keyword: "const", params: { allowedValue: "task" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err38];
                    } else {
                      vErrors.push(err38);
                    }
                    errors++;
                  }
                  var valid10 = _errs35 === errors;
                } else {
                  var valid10 = true;
                }
                if (valid10) {
                  if (data.projectId !== undefined) {
                    let data10 = data.projectId;
                    const _errs36 = errors;
                    const _errs37 = errors;
                    if (errors === _errs37) {
                      if (typeof data10 === "string") {
                        if (func3(data10) > 240) {
                          const err39 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                          if (vErrors === null) {
                            vErrors = [err39];
                          } else {
                            vErrors.push(err39);
                          }
                          errors++;
                        } else {
                          if (func3(data10) < 1) {
                            const err40 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                            if (vErrors === null) {
                              vErrors = [err40];
                            } else {
                              vErrors.push(err40);
                            }
                            errors++;
                          } else {
                            if (!pattern0.test(data10)) {
                              const err41 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                              if (vErrors === null) {
                                vErrors = [err41];
                              } else {
                                vErrors.push(err41);
                              }
                              errors++;
                            }
                          }
                        }
                      } else {
                        const err42 = { instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                        if (vErrors === null) {
                          vErrors = [err42];
                        } else {
                          vErrors.push(err42);
                        }
                        errors++;
                      }
                    }
                    var valid10 = _errs36 === errors;
                  } else {
                    var valid10 = true;
                  }
                  if (valid10) {
                    if (data.workId !== undefined) {
                      let data11 = data.workId;
                      const _errs39 = errors;
                      const _errs40 = errors;
                      if (errors === _errs40) {
                        if (typeof data11 === "string") {
                          if (func3(data11) > 240) {
                            const err43 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                            if (vErrors === null) {
                              vErrors = [err43];
                            } else {
                              vErrors.push(err43);
                            }
                            errors++;
                          } else {
                            if (func3(data11) < 1) {
                              const err44 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                              if (vErrors === null) {
                                vErrors = [err44];
                              } else {
                                vErrors.push(err44);
                              }
                              errors++;
                            } else {
                              if (!pattern0.test(data11)) {
                                const err45 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                                if (vErrors === null) {
                                  vErrors = [err45];
                                } else {
                                  vErrors.push(err45);
                                }
                                errors++;
                              }
                            }
                          }
                        } else {
                          const err46 = { instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                          if (vErrors === null) {
                            vErrors = [err46];
                          } else {
                            vErrors.push(err46);
                          }
                          errors++;
                        }
                      }
                      var valid10 = _errs39 === errors;
                    } else {
                      var valid10 = true;
                    }
                    if (valid10) {
                      if (data.taskId !== undefined) {
                        let data12 = data.taskId;
                        const _errs42 = errors;
                        const _errs43 = errors;
                        if (errors === _errs43) {
                          if (typeof data12 === "string") {
                            if (func3(data12) > 240) {
                              const err47 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                              if (vErrors === null) {
                                vErrors = [err47];
                              } else {
                                vErrors.push(err47);
                              }
                              errors++;
                            } else {
                              if (func3(data12) < 1) {
                                const err48 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                                if (vErrors === null) {
                                  vErrors = [err48];
                                } else {
                                  vErrors.push(err48);
                                }
                                errors++;
                              } else {
                                if (!pattern0.test(data12)) {
                                  const err49 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                                  if (vErrors === null) {
                                    vErrors = [err49];
                                  } else {
                                    vErrors.push(err49);
                                  }
                                  errors++;
                                }
                              }
                            }
                          } else {
                            const err50 = { instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                            if (vErrors === null) {
                              vErrors = [err50];
                            } else {
                              vErrors.push(err50);
                            }
                            errors++;
                          }
                        }
                        var valid10 = _errs42 === errors;
                      } else {
                        var valid10 = true;
                      }
                    }
                  }
                }
              }
            }
          } else {
            const err51 = { instancePath, schemaPath: "#/oneOf/4/type", keyword: "type", params: { type: "object" }, message: "must be object" };
            if (vErrors === null) {
              vErrors = [err51];
            } else {
              vErrors.push(err51);
            }
            errors++;
          }
        }
        var _valid0 = _errs32 === errors;
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
    const err52 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err52];
    } else {
      vErrors.push(err52);
    }
    errors++;
    validate69.errors = vErrors;
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
  validate69.errors = vErrors;
  return errors === 0;
}
function validate68(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.kind === undefined && (missing0 = "kind") || data.from === undefined && (missing0 = "from") || data.to === undefined && (missing0 = "to")) {
        validate68.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "kind" || key0 === "from" || key0 === "to")) {
            validate68.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.kind !== undefined) {
            const _errs2 = errors;
            if (data.kind !== "relatedTo") {
              validate68.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/const", keyword: "const", params: { allowedValue: "relatedTo" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.from !== undefined) {
              const _errs3 = errors;
              if (!validate69(data.from, { instancePath: instancePath + "/from", parentData: data, parentDataProperty: "from", rootData })) {
                vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.to !== undefined) {
                const _errs4 = errors;
                if (!validate69(data.to, { instancePath: instancePath + "/to", parentData: data, parentDataProperty: "to", rootData })) {
                  vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
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
      validate68.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate68.errors = vErrors;
  return errors === 0;
}
function validate73(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.from === undefined && (missing0 = "from") || data.to === undefined && (missing0 = "to") || data.authorityRef === undefined && (missing0 = "authorityRef")) {
        validate73.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "from" || key0 === "to" || key0 === "authorityRef")) {
            validate73.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.from !== undefined) {
            const _errs2 = errors;
            if (!validate69(data.from, { instancePath: instancePath + "/from", parentData: data, parentDataProperty: "from", rootData })) {
              vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.to !== undefined) {
              const _errs3 = errors;
              if (!validate69(data.to, { instancePath: instancePath + "/to", parentData: data, parentDataProperty: "to", rootData })) {
                vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.authorityRef !== undefined) {
                const _errs4 = errors;
                if (!validate37(data.authorityRef, { instancePath: instancePath + "/authorityRef", parentData: data, parentDataProperty: "authorityRef", rootData })) {
                  vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
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
      validate73.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate73.errors = vErrors;
  return errors === 0;
}
function validate78(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.subject === undefined && (missing0 = "subject") || data.source === undefined && (missing0 = "source")) {
        validate78.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "subject" || key0 === "source")) {
            validate78.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.subject !== undefined) {
            const _errs2 = errors;
            if (!validate69(data.subject, { instancePath: instancePath + "/subject", parentData: data, parentDataProperty: "subject", rootData })) {
              vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.source !== undefined) {
              const _errs3 = errors;
              if (!validate37(data.source, { instancePath: instancePath + "/source", parentData: data, parentDataProperty: "source", rootData })) {
                vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
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
      validate78.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate78.errors = vErrors;
  return errors === 0;
}
function validate84(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.runtimeId === undefined && (missing0 = "runtimeId") || data.nativeSessionId === undefined && (missing0 = "nativeSessionId") || data.nativeTaskId === undefined && (missing0 = "nativeTaskId") || data.nativeRunId === undefined && (missing0 = "nativeRunId") || data.paneId === undefined && (missing0 = "paneId")) {
        validate84.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "runtimeId" || key0 === "nativeSessionId" || key0 === "nativeTaskId" || key0 === "nativeRunId" || key0 === "paneId")) {
            validate84.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.runtimeId !== undefined) {
            let data0 = data.runtimeId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate84.errors = [{ instancePath: instancePath + "/runtimeId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate84.errors = [{ instancePath: instancePath + "/runtimeId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate84.errors = [{ instancePath: instancePath + "/runtimeId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate84.errors = [{ instancePath: instancePath + "/runtimeId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.nativeSessionId !== undefined) {
              let data1 = data.nativeSessionId;
              const _errs5 = errors;
              const _errs6 = errors;
              let valid2 = false;
              const _errs7 = errors;
              const _errs8 = errors;
              if (errors === _errs8) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    const err0 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                    if (vErrors === null) {
                      vErrors = [err0];
                    } else {
                      vErrors.push(err0);
                    }
                    errors++;
                  } else {
                    if (func3(data1) < 1) {
                      const err1 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                      if (vErrors === null) {
                        vErrors = [err1];
                      } else {
                        vErrors.push(err1);
                      }
                      errors++;
                    } else {
                      if (!pattern0.test(data1)) {
                        const err2 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                        if (vErrors === null) {
                          vErrors = [err2];
                        } else {
                          vErrors.push(err2);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err3 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err3];
                  } else {
                    vErrors.push(err3);
                  }
                  errors++;
                }
              }
              var _valid0 = _errs7 === errors;
              valid2 = valid2 || _valid0;
              if (!valid2) {
                const _errs10 = errors;
                if (data1 !== null) {
                  const err4 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/properties/nativeSessionId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                  if (vErrors === null) {
                    vErrors = [err4];
                  } else {
                    vErrors.push(err4);
                  }
                  errors++;
                }
                var _valid0 = _errs10 === errors;
                valid2 = valid2 || _valid0;
              }
              if (!valid2) {
                const err5 = { instancePath: instancePath + "/nativeSessionId", schemaPath: "#/properties/nativeSessionId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                if (vErrors === null) {
                  vErrors = [err5];
                } else {
                  vErrors.push(err5);
                }
                errors++;
                validate84.errors = vErrors;
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
              if (data.nativeTaskId !== undefined) {
                let data2 = data.nativeTaskId;
                const _errs12 = errors;
                const _errs13 = errors;
                let valid4 = false;
                const _errs14 = errors;
                const _errs15 = errors;
                if (errors === _errs15) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      const err6 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                      if (vErrors === null) {
                        vErrors = [err6];
                      } else {
                        vErrors.push(err6);
                      }
                      errors++;
                    } else {
                      if (func3(data2) < 1) {
                        const err7 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                        if (vErrors === null) {
                          vErrors = [err7];
                        } else {
                          vErrors.push(err7);
                        }
                        errors++;
                      } else {
                        if (!pattern0.test(data2)) {
                          const err8 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err8];
                          } else {
                            vErrors.push(err8);
                          }
                          errors++;
                        }
                      }
                    }
                  } else {
                    const err9 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                    if (vErrors === null) {
                      vErrors = [err9];
                    } else {
                      vErrors.push(err9);
                    }
                    errors++;
                  }
                }
                var _valid1 = _errs14 === errors;
                valid4 = valid4 || _valid1;
                if (!valid4) {
                  const _errs17 = errors;
                  if (data2 !== null) {
                    const err10 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/properties/nativeTaskId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                    if (vErrors === null) {
                      vErrors = [err10];
                    } else {
                      vErrors.push(err10);
                    }
                    errors++;
                  }
                  var _valid1 = _errs17 === errors;
                  valid4 = valid4 || _valid1;
                }
                if (!valid4) {
                  const err11 = { instancePath: instancePath + "/nativeTaskId", schemaPath: "#/properties/nativeTaskId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                  if (vErrors === null) {
                    vErrors = [err11];
                  } else {
                    vErrors.push(err11);
                  }
                  errors++;
                  validate84.errors = vErrors;
                  return false;
                } else {
                  errors = _errs13;
                  if (vErrors !== null) {
                    if (_errs13) {
                      vErrors.length = _errs13;
                    } else {
                      vErrors = null;
                    }
                  }
                }
                var valid0 = _errs12 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.nativeRunId !== undefined) {
                  let data3 = data.nativeRunId;
                  const _errs19 = errors;
                  const _errs20 = errors;
                  let valid6 = false;
                  const _errs21 = errors;
                  const _errs22 = errors;
                  if (errors === _errs22) {
                    if (typeof data3 === "string") {
                      if (func3(data3) > 240) {
                        const err12 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                        if (vErrors === null) {
                          vErrors = [err12];
                        } else {
                          vErrors.push(err12);
                        }
                        errors++;
                      } else {
                        if (func3(data3) < 1) {
                          const err13 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                          if (vErrors === null) {
                            vErrors = [err13];
                          } else {
                            vErrors.push(err13);
                          }
                          errors++;
                        } else {
                          if (!pattern0.test(data3)) {
                            const err14 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                            if (vErrors === null) {
                              vErrors = [err14];
                            } else {
                              vErrors.push(err14);
                            }
                            errors++;
                          }
                        }
                      }
                    } else {
                      const err15 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                      if (vErrors === null) {
                        vErrors = [err15];
                      } else {
                        vErrors.push(err15);
                      }
                      errors++;
                    }
                  }
                  var _valid2 = _errs21 === errors;
                  valid6 = valid6 || _valid2;
                  if (!valid6) {
                    const _errs24 = errors;
                    if (data3 !== null) {
                      const err16 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/properties/nativeRunId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                      if (vErrors === null) {
                        vErrors = [err16];
                      } else {
                        vErrors.push(err16);
                      }
                      errors++;
                    }
                    var _valid2 = _errs24 === errors;
                    valid6 = valid6 || _valid2;
                  }
                  if (!valid6) {
                    const err17 = { instancePath: instancePath + "/nativeRunId", schemaPath: "#/properties/nativeRunId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                    if (vErrors === null) {
                      vErrors = [err17];
                    } else {
                      vErrors.push(err17);
                    }
                    errors++;
                    validate84.errors = vErrors;
                    return false;
                  } else {
                    errors = _errs20;
                    if (vErrors !== null) {
                      if (_errs20) {
                        vErrors.length = _errs20;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                  var valid0 = _errs19 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.paneId !== undefined) {
                    let data4 = data.paneId;
                    const _errs26 = errors;
                    const _errs27 = errors;
                    let valid8 = false;
                    const _errs28 = errors;
                    const _errs29 = errors;
                    if (errors === _errs29) {
                      if (typeof data4 === "string") {
                        if (func3(data4) > 240) {
                          const err18 = { instancePath: instancePath + "/paneId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                          if (vErrors === null) {
                            vErrors = [err18];
                          } else {
                            vErrors.push(err18);
                          }
                          errors++;
                        } else {
                          if (func3(data4) < 1) {
                            const err19 = { instancePath: instancePath + "/paneId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                            if (vErrors === null) {
                              vErrors = [err19];
                            } else {
                              vErrors.push(err19);
                            }
                            errors++;
                          } else {
                            if (!pattern0.test(data4)) {
                              const err20 = { instancePath: instancePath + "/paneId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                              if (vErrors === null) {
                                vErrors = [err20];
                              } else {
                                vErrors.push(err20);
                              }
                              errors++;
                            }
                          }
                        }
                      } else {
                        const err21 = { instancePath: instancePath + "/paneId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                        if (vErrors === null) {
                          vErrors = [err21];
                        } else {
                          vErrors.push(err21);
                        }
                        errors++;
                      }
                    }
                    var _valid3 = _errs28 === errors;
                    valid8 = valid8 || _valid3;
                    if (!valid8) {
                      const _errs31 = errors;
                      if (data4 !== null) {
                        const err22 = { instancePath: instancePath + "/paneId", schemaPath: "#/properties/paneId/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                        if (vErrors === null) {
                          vErrors = [err22];
                        } else {
                          vErrors.push(err22);
                        }
                        errors++;
                      }
                      var _valid3 = _errs31 === errors;
                      valid8 = valid8 || _valid3;
                    }
                    if (!valid8) {
                      const err23 = { instancePath: instancePath + "/paneId", schemaPath: "#/properties/paneId/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                      if (vErrors === null) {
                        vErrors = [err23];
                      } else {
                        vErrors.push(err23);
                      }
                      errors++;
                      validate84.errors = vErrors;
                      return false;
                    } else {
                      errors = _errs27;
                      if (vErrors !== null) {
                        if (_errs27) {
                          vErrors.length = _errs27;
                        } else {
                          vErrors = null;
                        }
                      }
                    }
                    var valid0 = _errs26 === errors;
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
      validate84.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate84.errors = vErrors;
  return errors === 0;
}
function validate82(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.binding === undefined && (missing0 = "binding")) {
        validate82.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "binding")) {
            validate82.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate62(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.binding !== undefined) {
              const _errs3 = errors;
              if (!validate84(data.binding, { instancePath: instancePath + "/binding", parentData: data, parentDataProperty: "binding", rootData })) {
                vErrors = vErrors === null ? validate84.errors : vErrors.concat(validate84.errors);
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
      validate82.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate82.errors = vErrors;
  return errors === 0;
}
function validate88(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.domains === undefined && (missing0 = "domains") || data.projects === undefined && (missing0 = "projects") || data.works === undefined && (missing0 = "works") || data.tasks === undefined && (missing0 = "tasks") || data.contracts === undefined && (missing0 = "contracts") || data.dependencies === undefined && (missing0 = "dependencies") || data.relations === undefined && (missing0 = "relations") || data.delegations === undefined && (missing0 = "delegations") || data.provenance === undefined && (missing0 = "provenance") || data.nativeCorrelations === undefined && (missing0 = "nativeCorrelations")) {
        validate88.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema12.properties, key0)) {
            validate88.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work.v3") {
              validate88.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work.v3" }, message: "must be equal to constant" }];
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
                    validate88.errors = [{ instancePath: instancePath + "/domains", schemaPath: "#/properties/domains/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                    return false;
                  } else {
                    var valid1 = true;
                    const len0 = data1.length;
                    for (let i0 = 0;i0 < len0; i0++) {
                      const _errs5 = errors;
                      if (!validate12(data1[i0], { instancePath: instancePath + "/domains/" + i0, parentData: data1, parentDataProperty: i0, rootData })) {
                        vErrors = vErrors === null ? validate12.errors : vErrors.concat(validate12.errors);
                        errors = vErrors.length;
                      }
                      var valid1 = _errs5 === errors;
                      if (!valid1) {
                        break;
                      }
                    }
                  }
                } else {
                  validate88.errors = [{ instancePath: instancePath + "/domains", schemaPath: "#/properties/domains/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                      validate88.errors = [{ instancePath: instancePath + "/projects", schemaPath: "#/properties/projects/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                      return false;
                    } else {
                      var valid2 = true;
                      const len1 = data3.length;
                      for (let i1 = 0;i1 < len1; i1++) {
                        const _errs8 = errors;
                        if (!validate14(data3[i1], { instancePath: instancePath + "/projects/" + i1, parentData: data3, parentDataProperty: i1, rootData })) {
                          vErrors = vErrors === null ? validate14.errors : vErrors.concat(validate14.errors);
                          errors = vErrors.length;
                        }
                        var valid2 = _errs8 === errors;
                        if (!valid2) {
                          break;
                        }
                      }
                    }
                  } else {
                    validate88.errors = [{ instancePath: instancePath + "/projects", schemaPath: "#/properties/projects/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                        validate88.errors = [{ instancePath: instancePath + "/works", schemaPath: "#/properties/works/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                        return false;
                      } else {
                        var valid3 = true;
                        const len2 = data5.length;
                        for (let i2 = 0;i2 < len2; i2++) {
                          const _errs11 = errors;
                          if (!validate16(data5[i2], { instancePath: instancePath + "/works/" + i2, parentData: data5, parentDataProperty: i2, rootData })) {
                            vErrors = vErrors === null ? validate16.errors : vErrors.concat(validate16.errors);
                            errors = vErrors.length;
                          }
                          var valid3 = _errs11 === errors;
                          if (!valid3) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate88.errors = [{ instancePath: instancePath + "/works", schemaPath: "#/properties/works/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                          validate88.errors = [{ instancePath: instancePath + "/tasks", schemaPath: "#/properties/tasks/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                          return false;
                        } else {
                          var valid4 = true;
                          const len3 = data7.length;
                          for (let i3 = 0;i3 < len3; i3++) {
                            const _errs14 = errors;
                            if (!validate26(data7[i3], { instancePath: instancePath + "/tasks/" + i3, parentData: data7, parentDataProperty: i3, rootData })) {
                              vErrors = vErrors === null ? validate26.errors : vErrors.concat(validate26.errors);
                              errors = vErrors.length;
                            }
                            var valid4 = _errs14 === errors;
                            if (!valid4) {
                              break;
                            }
                          }
                        }
                      } else {
                        validate88.errors = [{ instancePath: instancePath + "/tasks", schemaPath: "#/properties/tasks/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                            validate88.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                            return false;
                          } else {
                            var valid5 = true;
                            const len4 = data9.length;
                            for (let i4 = 0;i4 < len4; i4++) {
                              const _errs17 = errors;
                              if (!validate35(data9[i4], { instancePath: instancePath + "/contracts/" + i4, parentData: data9, parentDataProperty: i4, rootData })) {
                                vErrors = vErrors === null ? validate35.errors : vErrors.concat(validate35.errors);
                                errors = vErrors.length;
                              }
                              var valid5 = _errs17 === errors;
                              if (!valid5) {
                                break;
                              }
                            }
                          }
                        } else {
                          validate88.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                              validate88.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                              return false;
                            } else {
                              var valid6 = true;
                              const len5 = data11.length;
                              for (let i5 = 0;i5 < len5; i5++) {
                                const _errs20 = errors;
                                if (!validate61(data11[i5], { instancePath: instancePath + "/dependencies/" + i5, parentData: data11, parentDataProperty: i5, rootData })) {
                                  vErrors = vErrors === null ? validate61.errors : vErrors.concat(validate61.errors);
                                  errors = vErrors.length;
                                }
                                var valid6 = _errs20 === errors;
                                if (!valid6) {
                                  break;
                                }
                              }
                            }
                          } else {
                            validate88.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                                validate88.errors = [{ instancePath: instancePath + "/relations", schemaPath: "#/properties/relations/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                return false;
                              } else {
                                var valid7 = true;
                                const len6 = data13.length;
                                for (let i6 = 0;i6 < len6; i6++) {
                                  const _errs23 = errors;
                                  if (!validate68(data13[i6], { instancePath: instancePath + "/relations/" + i6, parentData: data13, parentDataProperty: i6, rootData })) {
                                    vErrors = vErrors === null ? validate68.errors : vErrors.concat(validate68.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid7 = _errs23 === errors;
                                  if (!valid7) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate88.errors = [{ instancePath: instancePath + "/relations", schemaPath: "#/properties/relations/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                                  validate88.errors = [{ instancePath: instancePath + "/delegations", schemaPath: "#/properties/delegations/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                  return false;
                                } else {
                                  var valid8 = true;
                                  const len7 = data15.length;
                                  for (let i7 = 0;i7 < len7; i7++) {
                                    const _errs26 = errors;
                                    if (!validate73(data15[i7], { instancePath: instancePath + "/delegations/" + i7, parentData: data15, parentDataProperty: i7, rootData })) {
                                      vErrors = vErrors === null ? validate73.errors : vErrors.concat(validate73.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid8 = _errs26 === errors;
                                    if (!valid8) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate88.errors = [{ instancePath: instancePath + "/delegations", schemaPath: "#/properties/delegations/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                                    validate88.errors = [{ instancePath: instancePath + "/provenance", schemaPath: "#/properties/provenance/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                    return false;
                                  } else {
                                    var valid9 = true;
                                    const len8 = data17.length;
                                    for (let i8 = 0;i8 < len8; i8++) {
                                      const _errs29 = errors;
                                      if (!validate78(data17[i8], { instancePath: instancePath + "/provenance/" + i8, parentData: data17, parentDataProperty: i8, rootData })) {
                                        vErrors = vErrors === null ? validate78.errors : vErrors.concat(validate78.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid9 = _errs29 === errors;
                                      if (!valid9) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate88.errors = [{ instancePath: instancePath + "/provenance", schemaPath: "#/properties/provenance/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                                      validate88.errors = [{ instancePath: instancePath + "/nativeCorrelations", schemaPath: "#/properties/nativeCorrelations/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                      return false;
                                    } else {
                                      var valid10 = true;
                                      const len9 = data19.length;
                                      for (let i9 = 0;i9 < len9; i9++) {
                                        const _errs32 = errors;
                                        if (!validate82(data19[i9], { instancePath: instancePath + "/nativeCorrelations/" + i9, parentData: data19, parentDataProperty: i9, rootData })) {
                                          vErrors = vErrors === null ? validate82.errors : vErrors.concat(validate82.errors);
                                          errors = vErrors.length;
                                        }
                                        var valid10 = _errs32 === errors;
                                        if (!valid10) {
                                          break;
                                        }
                                      }
                                    }
                                  } else {
                                    validate88.errors = [{ instancePath: instancePath + "/nativeCorrelations", schemaPath: "#/properties/nativeCorrelations/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs30 === errors;
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
    } else {
      validate88.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate88.errors = vErrors;
  return errors === 0;
}
var validateWork = validate99;
function validate99(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.title === undefined && (missing0 = "title") || data.leadActorId === undefined && (missing0 = "leadActorId") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts") || data.status === undefined && (missing0 = "status")) {
        validate99.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId" || key0 === "title" || key0 === "leadActorId" || key0 === "revision" || key0 === "contracts" || key0 === "status")) {
            validate99.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            let data0 = data.projectId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate99.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate99.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate99.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate99.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              let data1 = data.workId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate99.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate99.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate99.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate99.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.title !== undefined) {
                let data2 = data.title;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate99.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate99.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate99.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate99.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs8 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.leadActorId !== undefined) {
                  let data3 = data.leadActorId;
                  const _errs11 = errors;
                  const _errs12 = errors;
                  if (errors === _errs12) {
                    if (typeof data3 === "string") {
                      if (func3(data3) > 240) {
                        validate99.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                        return false;
                      } else {
                        if (func3(data3) < 1) {
                          validate99.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        } else {
                          if (!pattern0.test(data3)) {
                            validate99.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                            return false;
                          }
                        }
                      }
                    } else {
                      validate99.errors = [{ instancePath: instancePath + "/leadActorId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs11 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.revision !== undefined) {
                    let data4 = data.revision;
                    const _errs14 = errors;
                    const _errs15 = errors;
                    if (!(typeof data4 == "number" && (!(data4 % 1) && !isNaN(data4)) && isFinite(data4))) {
                      validate99.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                      return false;
                    }
                    if (errors === _errs15) {
                      if (typeof data4 == "number" && isFinite(data4)) {
                        if (data4 > 9007199254740991 || isNaN(data4)) {
                          validate99.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                          return false;
                        } else {
                          if (data4 < 1 || isNaN(data4)) {
                            validate99.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                            return false;
                          }
                        }
                      }
                    }
                    var valid0 = _errs14 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.contracts !== undefined) {
                      const _errs17 = errors;
                      if (!validate17(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                        vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs17 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.status !== undefined) {
                        let data6 = data.status;
                        const _errs18 = errors;
                        if (!(data6 === "proposed" || data6 === "admitted" || data6 === "active" || data6 === "verified" || data6 === "accepted" || data6 === "cancelled")) {
                          validate99.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema21.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
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
        }
      }
    } else {
      validate99.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate99.errors = vErrors;
  return errors === 0;
}
var validateTask = validate101;
function validate101(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.taskId === undefined && (missing0 = "taskId") || data.title === undefined && (missing0 = "title") || data.assigneeActorId === undefined && (missing0 = "assigneeActorId") || data.revision === undefined && (missing0 = "revision") || data.contractOverrides === undefined && (missing0 = "contractOverrides") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.resources === undefined && (missing0 = "resources") || data.status === undefined && (missing0 = "status")) {
        validate101.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema38.properties, key0)) {
            validate101.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.projectId !== undefined) {
            let data0 = data.projectId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate101.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate101.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate101.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate101.errors = [{ instancePath: instancePath + "/projectId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.workId !== undefined) {
              let data1 = data.workId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate101.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate101.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate101.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate101.errors = [{ instancePath: instancePath + "/workId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.taskId !== undefined) {
                let data2 = data.taskId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate101.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate101.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate101.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate101.errors = [{ instancePath: instancePath + "/taskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs8 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.title !== undefined) {
                  let data3 = data.title;
                  const _errs11 = errors;
                  const _errs12 = errors;
                  if (errors === _errs12) {
                    if (typeof data3 === "string") {
                      if (func3(data3) > 240) {
                        validate101.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                        return false;
                      } else {
                        if (func3(data3) < 1) {
                          validate101.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        } else {
                          if (!pattern0.test(data3)) {
                            validate101.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                            return false;
                          }
                        }
                      }
                    } else {
                      validate101.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs11 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.parentTaskId !== undefined) {
                    let data4 = data.parentTaskId;
                    const _errs14 = errors;
                    const _errs15 = errors;
                    if (errors === _errs15) {
                      if (typeof data4 === "string") {
                        if (func3(data4) > 240) {
                          validate101.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                          return false;
                        } else {
                          if (func3(data4) < 1) {
                            validate101.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                            return false;
                          } else {
                            if (!pattern0.test(data4)) {
                              validate101.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                              return false;
                            }
                          }
                        }
                      } else {
                        validate101.errors = [{ instancePath: instancePath + "/parentTaskId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid0 = _errs14 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.assigneeActorId !== undefined) {
                      let data5 = data.assigneeActorId;
                      const _errs17 = errors;
                      const _errs18 = errors;
                      if (errors === _errs18) {
                        if (typeof data5 === "string") {
                          if (func3(data5) > 240) {
                            validate101.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                            return false;
                          } else {
                            if (func3(data5) < 1) {
                              validate101.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            } else {
                              if (!pattern0.test(data5)) {
                                validate101.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                return false;
                              }
                            }
                          }
                        } else {
                          validate101.errors = [{ instancePath: instancePath + "/assigneeActorId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid0 = _errs17 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.revision !== undefined) {
                        let data6 = data.revision;
                        const _errs20 = errors;
                        const _errs21 = errors;
                        if (!(typeof data6 == "number" && (!(data6 % 1) && !isNaN(data6)) && isFinite(data6))) {
                          validate101.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                          return false;
                        }
                        if (errors === _errs21) {
                          if (typeof data6 == "number" && isFinite(data6)) {
                            if (data6 > 9007199254740991 || isNaN(data6)) {
                              validate101.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                              return false;
                            } else {
                              if (data6 < 1 || isNaN(data6)) {
                                validate101.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                return false;
                              }
                            }
                          }
                        }
                        var valid0 = _errs20 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.contractOverrides !== undefined) {
                          const _errs23 = errors;
                          if (!validate27(data.contractOverrides, { instancePath: instancePath + "/contractOverrides", parentData: data, parentDataProperty: "contractOverrides", rootData })) {
                            vErrors = vErrors === null ? validate27.errors : vErrors.concat(validate27.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs23 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.acceptanceCriteria !== undefined) {
                            let data8 = data.acceptanceCriteria;
                            const _errs24 = errors;
                            if (errors === _errs24) {
                              if (Array.isArray(data8)) {
                                if (data8.length > 2000) {
                                  validate101.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid8 = true;
                                  const len0 = data8.length;
                                  for (let i0 = 0;i0 < len0; i0++) {
                                    let data9 = data8[i0];
                                    const _errs26 = errors;
                                    const _errs27 = errors;
                                    if (errors === _errs27) {
                                      if (typeof data9 === "string") {
                                        if (func3(data9) > 240) {
                                          validate101.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                          return false;
                                        } else {
                                          if (func3(data9) < 1) {
                                            validate101.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                            return false;
                                          } else {
                                            if (!pattern0.test(data9)) {
                                              validate101.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                              return false;
                                            }
                                          }
                                        }
                                      } else {
                                        validate101.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i0, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid8 = _errs26 === errors;
                                    if (!valid8) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate101.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs24 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.resources !== undefined) {
                              let data10 = data.resources;
                              const _errs29 = errors;
                              if (errors === _errs29) {
                                if (Array.isArray(data10)) {
                                  if (data10.length > 2000) {
                                    validate101.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid10 = true;
                                    const len1 = data10.length;
                                    for (let i1 = 0;i1 < len1; i1++) {
                                      const _errs31 = errors;
                                      if (!validate32(data10[i1], { instancePath: instancePath + "/resources/" + i1, parentData: data10, parentDataProperty: i1, rootData })) {
                                        vErrors = vErrors === null ? validate32.errors : vErrors.concat(validate32.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid10 = _errs31 === errors;
                                      if (!valid10) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate101.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs29 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.status !== undefined) {
                                let data12 = data.status;
                                const _errs32 = errors;
                                if (!(data12 === "planned" || data12 === "ready" || data12 === "running" || data12 === "blocked" || data12 === "verified" || data12 === "accepted" || data12 === "cancelled")) {
                                  validate101.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema38.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid0 = _errs32 === errors;
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
    } else {
      validate101.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate101.errors = vErrors;
  return errors === 0;
}
var validateRun = validate104;
var schema115 = { type: "object", properties: { runId: { $ref: "#/$defs/Id" }, task: { $ref: "#/$defs/TaskRef" }, attempt: { $ref: "#/$defs/Revision" }, status: { enum: ["reserved", "started", "succeeded", "failed", "cancelled", "uncertain"] }, effectiveContractDigest: { $ref: "#/$defs/Digest" }, sourceSnapshotId: { $ref: "#/$defs/Id" }, parentAuthorityRef: { $ref: "#/$defs/Id" }, nativeBindings: { type: "array", items: { $ref: "#/$defs/NativeBinding" }, maxItems: 2000 }, effectRefs: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2000 } }, required: ["runId", "task", "attempt", "status", "effectiveContractDigest", "sourceSnapshotId", "parentAuthorityRef", "nativeBindings", "effectRefs"], additionalProperties: false };
function validate104(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.runId === undefined && (missing0 = "runId") || data.task === undefined && (missing0 = "task") || data.attempt === undefined && (missing0 = "attempt") || data.status === undefined && (missing0 = "status") || data.effectiveContractDigest === undefined && (missing0 = "effectiveContractDigest") || data.sourceSnapshotId === undefined && (missing0 = "sourceSnapshotId") || data.parentAuthorityRef === undefined && (missing0 = "parentAuthorityRef") || data.nativeBindings === undefined && (missing0 = "nativeBindings") || data.effectRefs === undefined && (missing0 = "effectRefs")) {
        validate104.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema115.properties, key0)) {
            validate104.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.runId !== undefined) {
            let data0 = data.runId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate104.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate104.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate104.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate104.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs5 = errors;
              if (!validate62(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.attempt !== undefined) {
                let data2 = data.attempt;
                const _errs6 = errors;
                const _errs7 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  validate104.errors = [{ instancePath: instancePath + "/attempt", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                  return false;
                }
                if (errors === _errs7) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      validate104.errors = [{ instancePath: instancePath + "/attempt", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                      return false;
                    } else {
                      if (data2 < 1 || isNaN(data2)) {
                        validate104.errors = [{ instancePath: instancePath + "/attempt", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                        return false;
                      }
                    }
                  }
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.status !== undefined) {
                  let data3 = data.status;
                  const _errs9 = errors;
                  if (!(data3 === "reserved" || data3 === "started" || data3 === "succeeded" || data3 === "failed" || data3 === "cancelled" || data3 === "uncertain")) {
                    validate104.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema115.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs9 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.effectiveContractDigest !== undefined) {
                    let data4 = data.effectiveContractDigest;
                    const _errs10 = errors;
                    const _errs11 = errors;
                    if (errors === _errs11) {
                      if (typeof data4 === "string") {
                        if (!pattern16.test(data4)) {
                          validate104.errors = [{ instancePath: instancePath + "/effectiveContractDigest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                          return false;
                        }
                      } else {
                        validate104.errors = [{ instancePath: instancePath + "/effectiveContractDigest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.sourceSnapshotId !== undefined) {
                      let data5 = data.sourceSnapshotId;
                      const _errs13 = errors;
                      const _errs14 = errors;
                      if (errors === _errs14) {
                        if (typeof data5 === "string") {
                          if (func3(data5) > 240) {
                            validate104.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                            return false;
                          } else {
                            if (func3(data5) < 1) {
                              validate104.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            } else {
                              if (!pattern0.test(data5)) {
                                validate104.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                return false;
                              }
                            }
                          }
                        } else {
                          validate104.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid0 = _errs13 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.parentAuthorityRef !== undefined) {
                        let data6 = data.parentAuthorityRef;
                        const _errs16 = errors;
                        const _errs17 = errors;
                        if (errors === _errs17) {
                          if (typeof data6 === "string") {
                            if (func3(data6) > 240) {
                              validate104.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                              return false;
                            } else {
                              if (func3(data6) < 1) {
                                validate104.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                return false;
                              } else {
                                if (!pattern0.test(data6)) {
                                  validate104.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                  return false;
                                }
                              }
                            }
                          } else {
                            validate104.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                        }
                        var valid0 = _errs16 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.nativeBindings !== undefined) {
                          let data7 = data.nativeBindings;
                          const _errs19 = errors;
                          if (errors === _errs19) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate104.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid6 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs21 = errors;
                                  if (!validate84(data7[i0], { instancePath: instancePath + "/nativeBindings/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate84.errors : vErrors.concat(validate84.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid6 = _errs21 === errors;
                                  if (!valid6) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate104.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs19 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.effectRefs !== undefined) {
                            let data9 = data.effectRefs;
                            const _errs22 = errors;
                            if (errors === _errs22) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate104.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid7 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    let data10 = data9[i1];
                                    const _errs24 = errors;
                                    const _errs25 = errors;
                                    if (errors === _errs25) {
                                      if (typeof data10 === "string") {
                                        if (func3(data10) > 240) {
                                          validate104.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                          return false;
                                        } else {
                                          if (func3(data10) < 1) {
                                            validate104.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                            return false;
                                          } else {
                                            if (!pattern0.test(data10)) {
                                              validate104.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                              return false;
                                            }
                                          }
                                        }
                                      } else {
                                        validate104.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid7 = _errs24 === errors;
                                    if (!valid7) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate104.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs22 === errors;
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
      validate104.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate104.errors = vErrors;
  return errors === 0;
}
var validateContract = validate107;
function validate107(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
            let data0 = data.id;
            const _errs4 = errors;
            const _errs5 = errors;
            if (errors === _errs5) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  const err2 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                  if (vErrors === null) {
                    vErrors = [err2];
                  } else {
                    vErrors.push(err2);
                  }
                  errors++;
                } else {
                  if (func3(data0) < 1) {
                    const err3 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                    if (vErrors === null) {
                      vErrors = [err3];
                    } else {
                      vErrors.push(err3);
                    }
                    errors++;
                  } else {
                    if (!pattern0.test(data0)) {
                      const err4 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                      if (vErrors === null) {
                        vErrors = [err4];
                      } else {
                        vErrors.push(err4);
                      }
                      errors++;
                    }
                  }
                }
              } else {
                const err5 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                if (vErrors === null) {
                  vErrors = [err5];
                } else {
                  vErrors.push(err5);
                }
                errors++;
              }
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.owner !== undefined) {
              const _errs7 = errors;
              if (!validate19(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate19.errors : vErrors.concat(validate19.errors);
                errors = vErrors.length;
              }
              var valid1 = _errs7 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.revision !== undefined) {
                let data2 = data.revision;
                const _errs8 = errors;
                const _errs9 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  const err6 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
                  if (vErrors === null) {
                    vErrors = [err6];
                  } else {
                    vErrors.push(err6);
                  }
                  errors++;
                }
                if (errors === _errs9) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      const err7 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
                      if (vErrors === null) {
                        vErrors = [err7];
                      } else {
                        vErrors.push(err7);
                      }
                      errors++;
                    } else {
                      if (data2 < 1 || isNaN(data2)) {
                        const err8 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" };
                        if (vErrors === null) {
                          vErrors = [err8];
                        } else {
                          vErrors.push(err8);
                        }
                        errors++;
                      }
                    }
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
              if (valid1) {
                if (data.kind !== undefined) {
                  const _errs11 = errors;
                  if (data.kind !== "issue") {
                    const err9 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/0/properties/kind/const", keyword: "const", params: { allowedValue: "issue" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err9];
                    } else {
                      vErrors.push(err9);
                    }
                    errors++;
                  }
                  var valid1 = _errs11 === errors;
                } else {
                  var valid1 = true;
                }
                if (valid1) {
                  if (data.digest !== undefined) {
                    let data4 = data.digest;
                    const _errs12 = errors;
                    const _errs13 = errors;
                    if (errors === _errs13) {
                      if (typeof data4 === "string") {
                        if (!pattern16.test(data4)) {
                          const err10 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err10];
                          } else {
                            vErrors.push(err10);
                          }
                          errors++;
                        }
                      } else {
                        const err11 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                        if (vErrors === null) {
                          vErrors = [err11];
                        } else {
                          vErrors.push(err11);
                        }
                        errors++;
                      }
                    }
                    var valid1 = _errs12 === errors;
                  } else {
                    var valid1 = true;
                  }
                  if (valid1) {
                    if (data.sourceRefs !== undefined) {
                      let data5 = data.sourceRefs;
                      const _errs15 = errors;
                      if (errors === _errs15) {
                        if (Array.isArray(data5)) {
                          if (data5.length > 2000) {
                            const err12 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err12];
                            } else {
                              vErrors.push(err12);
                            }
                            errors++;
                          } else {
                            var valid5 = true;
                            const len0 = data5.length;
                            for (let i0 = 0;i0 < len0; i0++) {
                              const _errs17 = errors;
                              if (!validate37(data5[i0], { instancePath: instancePath + "/sourceRefs/" + i0, parentData: data5, parentDataProperty: i0, rootData })) {
                                vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
                                errors = vErrors.length;
                              }
                              var valid5 = _errs17 === errors;
                              if (!valid5) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err13 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/0/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err13];
                          } else {
                            vErrors.push(err13);
                          }
                          errors++;
                        }
                      }
                      var valid1 = _errs15 === errors;
                    } else {
                      var valid1 = true;
                    }
                    if (valid1) {
                      if (data.body !== undefined) {
                        const _errs18 = errors;
                        if (!validate39(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                          errors = vErrors.length;
                        }
                        var valid1 = _errs18 === errors;
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
      const err14 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err14];
      } else {
        vErrors.push(err14);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs19 = errors;
  if (errors === _errs19) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.id === undefined && (missing1 = "id") || data.owner === undefined && (missing1 = "owner") || data.revision === undefined && (missing1 = "revision") || data.kind === undefined && (missing1 = "kind") || data.digest === undefined && (missing1 = "digest") || data.sourceRefs === undefined && (missing1 = "sourceRefs") || data.body === undefined && (missing1 = "body")) {
        const err15 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err15];
        } else {
          vErrors.push(err15);
        }
        errors++;
      } else {
        const _errs21 = errors;
        for (const key1 in data) {
          if (!(key1 === "id" || key1 === "owner" || key1 === "revision" || key1 === "kind" || key1 === "digest" || key1 === "sourceRefs" || key1 === "body")) {
            const err16 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err16];
            } else {
              vErrors.push(err16);
            }
            errors++;
            break;
          }
        }
        if (_errs21 === errors) {
          if (data.id !== undefined) {
            let data8 = data.id;
            const _errs22 = errors;
            const _errs23 = errors;
            if (errors === _errs23) {
              if (typeof data8 === "string") {
                if (func3(data8) > 240) {
                  const err17 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                  if (vErrors === null) {
                    vErrors = [err17];
                  } else {
                    vErrors.push(err17);
                  }
                  errors++;
                } else {
                  if (func3(data8) < 1) {
                    const err18 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                    if (vErrors === null) {
                      vErrors = [err18];
                    } else {
                      vErrors.push(err18);
                    }
                    errors++;
                  } else {
                    if (!pattern0.test(data8)) {
                      const err19 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                      if (vErrors === null) {
                        vErrors = [err19];
                      } else {
                        vErrors.push(err19);
                      }
                      errors++;
                    }
                  }
                }
              } else {
                const err20 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                if (vErrors === null) {
                  vErrors = [err20];
                } else {
                  vErrors.push(err20);
                }
                errors++;
              }
            }
            var valid6 = _errs22 === errors;
          } else {
            var valid6 = true;
          }
          if (valid6) {
            if (data.owner !== undefined) {
              const _errs25 = errors;
              if (!validate19(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                vErrors = vErrors === null ? validate19.errors : vErrors.concat(validate19.errors);
                errors = vErrors.length;
              }
              var valid6 = _errs25 === errors;
            } else {
              var valid6 = true;
            }
            if (valid6) {
              if (data.revision !== undefined) {
                let data10 = data.revision;
                const _errs26 = errors;
                const _errs27 = errors;
                if (!(typeof data10 == "number" && (!(data10 % 1) && !isNaN(data10)) && isFinite(data10))) {
                  const err21 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
                  if (vErrors === null) {
                    vErrors = [err21];
                  } else {
                    vErrors.push(err21);
                  }
                  errors++;
                }
                if (errors === _errs27) {
                  if (typeof data10 == "number" && isFinite(data10)) {
                    if (data10 > 9007199254740991 || isNaN(data10)) {
                      const err22 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
                      if (vErrors === null) {
                        vErrors = [err22];
                      } else {
                        vErrors.push(err22);
                      }
                      errors++;
                    } else {
                      if (data10 < 1 || isNaN(data10)) {
                        const err23 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" };
                        if (vErrors === null) {
                          vErrors = [err23];
                        } else {
                          vErrors.push(err23);
                        }
                        errors++;
                      }
                    }
                  }
                }
                var valid6 = _errs26 === errors;
              } else {
                var valid6 = true;
              }
              if (valid6) {
                if (data.kind !== undefined) {
                  const _errs29 = errors;
                  if (data.kind !== "spec") {
                    const err24 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "spec" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err24];
                    } else {
                      vErrors.push(err24);
                    }
                    errors++;
                  }
                  var valid6 = _errs29 === errors;
                } else {
                  var valid6 = true;
                }
                if (valid6) {
                  if (data.digest !== undefined) {
                    let data12 = data.digest;
                    const _errs30 = errors;
                    const _errs31 = errors;
                    if (errors === _errs31) {
                      if (typeof data12 === "string") {
                        if (!pattern16.test(data12)) {
                          const err25 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' };
                          if (vErrors === null) {
                            vErrors = [err25];
                          } else {
                            vErrors.push(err25);
                          }
                          errors++;
                        }
                      } else {
                        const err26 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                        if (vErrors === null) {
                          vErrors = [err26];
                        } else {
                          vErrors.push(err26);
                        }
                        errors++;
                      }
                    }
                    var valid6 = _errs30 === errors;
                  } else {
                    var valid6 = true;
                  }
                  if (valid6) {
                    if (data.sourceRefs !== undefined) {
                      let data13 = data.sourceRefs;
                      const _errs33 = errors;
                      if (errors === _errs33) {
                        if (Array.isArray(data13)) {
                          if (data13.length > 2000) {
                            const err27 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                            if (vErrors === null) {
                              vErrors = [err27];
                            } else {
                              vErrors.push(err27);
                            }
                            errors++;
                          } else {
                            var valid10 = true;
                            const len1 = data13.length;
                            for (let i1 = 0;i1 < len1; i1++) {
                              const _errs35 = errors;
                              if (!validate37(data13[i1], { instancePath: instancePath + "/sourceRefs/" + i1, parentData: data13, parentDataProperty: i1, rootData })) {
                                vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
                                errors = vErrors.length;
                              }
                              var valid10 = _errs35 === errors;
                              if (!valid10) {
                                break;
                              }
                            }
                          }
                        } else {
                          const err28 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/1/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                          if (vErrors === null) {
                            vErrors = [err28];
                          } else {
                            vErrors.push(err28);
                          }
                          errors++;
                        }
                      }
                      var valid6 = _errs33 === errors;
                    } else {
                      var valid6 = true;
                    }
                    if (valid6) {
                      if (data.body !== undefined) {
                        const _errs36 = errors;
                        if (!validate46(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                          vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
                          errors = vErrors.length;
                        }
                        var valid6 = _errs36 === errors;
                      } else {
                        var valid6 = true;
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
      const err29 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err29];
      } else {
        vErrors.push(err29);
      }
      errors++;
    }
  }
  var _valid0 = _errs19 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
    const _errs37 = errors;
    if (errors === _errs37) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.id === undefined && (missing2 = "id") || data.owner === undefined && (missing2 = "owner") || data.revision === undefined && (missing2 = "revision") || data.kind === undefined && (missing2 = "kind") || data.digest === undefined && (missing2 = "digest") || data.sourceRefs === undefined && (missing2 = "sourceRefs") || data.body === undefined && (missing2 = "body")) {
          const err30 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err30];
          } else {
            vErrors.push(err30);
          }
          errors++;
        } else {
          const _errs39 = errors;
          for (const key2 in data) {
            if (!(key2 === "id" || key2 === "owner" || key2 === "revision" || key2 === "kind" || key2 === "digest" || key2 === "sourceRefs" || key2 === "body")) {
              const err31 = { instancePath, schemaPath: "#/oneOf/2/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" };
              if (vErrors === null) {
                vErrors = [err31];
              } else {
                vErrors.push(err31);
              }
              errors++;
              break;
            }
          }
          if (_errs39 === errors) {
            if (data.id !== undefined) {
              let data16 = data.id;
              const _errs40 = errors;
              const _errs41 = errors;
              if (errors === _errs41) {
                if (typeof data16 === "string") {
                  if (func3(data16) > 240) {
                    const err32 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" };
                    if (vErrors === null) {
                      vErrors = [err32];
                    } else {
                      vErrors.push(err32);
                    }
                    errors++;
                  } else {
                    if (func3(data16) < 1) {
                      const err33 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                      if (vErrors === null) {
                        vErrors = [err33];
                      } else {
                        vErrors.push(err33);
                      }
                      errors++;
                    } else {
                      if (!pattern0.test(data16)) {
                        const err34 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' };
                        if (vErrors === null) {
                          vErrors = [err34];
                        } else {
                          vErrors.push(err34);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err35 = { instancePath: instancePath + "/id", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err35];
                  } else {
                    vErrors.push(err35);
                  }
                  errors++;
                }
              }
              var valid11 = _errs40 === errors;
            } else {
              var valid11 = true;
            }
            if (valid11) {
              if (data.owner !== undefined) {
                const _errs43 = errors;
                if (!validate19(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                  vErrors = vErrors === null ? validate19.errors : vErrors.concat(validate19.errors);
                  errors = vErrors.length;
                }
                var valid11 = _errs43 === errors;
              } else {
                var valid11 = true;
              }
              if (valid11) {
                if (data.revision !== undefined) {
                  let data18 = data.revision;
                  const _errs44 = errors;
                  const _errs45 = errors;
                  if (!(typeof data18 == "number" && (!(data18 % 1) && !isNaN(data18)) && isFinite(data18))) {
                    const err36 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
                    if (vErrors === null) {
                      vErrors = [err36];
                    } else {
                      vErrors.push(err36);
                    }
                    errors++;
                  }
                  if (errors === _errs45) {
                    if (typeof data18 == "number" && isFinite(data18)) {
                      if (data18 > 9007199254740991 || isNaN(data18)) {
                        const err37 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
                        if (vErrors === null) {
                          vErrors = [err37];
                        } else {
                          vErrors.push(err37);
                        }
                        errors++;
                      } else {
                        if (data18 < 1 || isNaN(data18)) {
                          const err38 = { instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" };
                          if (vErrors === null) {
                            vErrors = [err38];
                          } else {
                            vErrors.push(err38);
                          }
                          errors++;
                        }
                      }
                    }
                  }
                  var valid11 = _errs44 === errors;
                } else {
                  var valid11 = true;
                }
                if (valid11) {
                  if (data.kind !== undefined) {
                    const _errs47 = errors;
                    if (data.kind !== "plan") {
                      const err39 = { instancePath: instancePath + "/kind", schemaPath: "#/oneOf/2/properties/kind/const", keyword: "const", params: { allowedValue: "plan" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err39];
                      } else {
                        vErrors.push(err39);
                      }
                      errors++;
                    }
                    var valid11 = _errs47 === errors;
                  } else {
                    var valid11 = true;
                  }
                  if (valid11) {
                    if (data.digest !== undefined) {
                      let data20 = data.digest;
                      const _errs48 = errors;
                      const _errs49 = errors;
                      if (errors === _errs49) {
                        if (typeof data20 === "string") {
                          if (!pattern16.test(data20)) {
                            const err40 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' };
                            if (vErrors === null) {
                              vErrors = [err40];
                            } else {
                              vErrors.push(err40);
                            }
                            errors++;
                          }
                        } else {
                          const err41 = { instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                          if (vErrors === null) {
                            vErrors = [err41];
                          } else {
                            vErrors.push(err41);
                          }
                          errors++;
                        }
                      }
                      var valid11 = _errs48 === errors;
                    } else {
                      var valid11 = true;
                    }
                    if (valid11) {
                      if (data.sourceRefs !== undefined) {
                        let data21 = data.sourceRefs;
                        const _errs51 = errors;
                        if (errors === _errs51) {
                          if (Array.isArray(data21)) {
                            if (data21.length > 2000) {
                              const err42 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" };
                              if (vErrors === null) {
                                vErrors = [err42];
                              } else {
                                vErrors.push(err42);
                              }
                              errors++;
                            } else {
                              var valid15 = true;
                              const len2 = data21.length;
                              for (let i2 = 0;i2 < len2; i2++) {
                                const _errs53 = errors;
                                if (!validate37(data21[i2], { instancePath: instancePath + "/sourceRefs/" + i2, parentData: data21, parentDataProperty: i2, rootData })) {
                                  vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
                                  errors = vErrors.length;
                                }
                                var valid15 = _errs53 === errors;
                                if (!valid15) {
                                  break;
                                }
                              }
                            }
                          } else {
                            const err43 = { instancePath: instancePath + "/sourceRefs", schemaPath: "#/oneOf/2/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" };
                            if (vErrors === null) {
                              vErrors = [err43];
                            } else {
                              vErrors.push(err43);
                            }
                            errors++;
                          }
                        }
                        var valid11 = _errs51 === errors;
                      } else {
                        var valid11 = true;
                      }
                      if (valid11) {
                        if (data.body !== undefined) {
                          const _errs54 = errors;
                          if (!validate55(data.body, { instancePath: instancePath + "/body", parentData: data, parentDataProperty: "body", rootData })) {
                            vErrors = vErrors === null ? validate55.errors : vErrors.concat(validate55.errors);
                            errors = vErrors.length;
                          }
                          var valid11 = _errs54 === errors;
                        } else {
                          var valid11 = true;
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
        const err44 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err44];
        } else {
          vErrors.push(err44);
        }
        errors++;
      }
    }
    var _valid0 = _errs37 === errors;
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
    const err45 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err45];
    } else {
      vErrors.push(err45);
    }
    errors++;
    validate107.errors = vErrors;
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
  validate107.errors = vErrors;
  return errors === 0;
}
var validateCreateRequest = validate117;
var schema132 = { type: "object", properties: { schema: { const: "common.git-work-create.v3" }, ownerId: { $ref: "#/$defs/Id" }, createRequestId: { $ref: "#/$defs/Id" }, kind: { enum: ["work", "task", "run"] }, scope: { $ref: "#/$defs/Scope" }, record: { $ref: "#/$defs/Json" }, contracts: { type: "array", items: { $ref: "#/$defs/Json" }, maxItems: 100 } }, required: ["schema", "ownerId", "createRequestId", "kind", "scope", "record", "contracts"], additionalProperties: false };
function validate117(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.createRequestId === undefined && (missing0 = "createRequestId") || data.kind === undefined && (missing0 = "kind") || data.scope === undefined && (missing0 = "scope") || data.record === undefined && (missing0 = "record") || data.contracts === undefined && (missing0 = "contracts")) {
        validate117.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "ownerId" || key0 === "createRequestId" || key0 === "kind" || key0 === "scope" || key0 === "record" || key0 === "contracts")) {
            validate117.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-create.v3") {
              validate117.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-create.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              let data1 = data.ownerId;
              const _errs3 = errors;
              const _errs4 = errors;
              if (errors === _errs4) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate117.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate117.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate117.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate117.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.createRequestId !== undefined) {
                let data2 = data.createRequestId;
                const _errs6 = errors;
                const _errs7 = errors;
                if (errors === _errs7) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate117.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate117.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate117.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate117.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.kind !== undefined) {
                  let data3 = data.kind;
                  const _errs9 = errors;
                  if (!(data3 === "work" || data3 === "task" || data3 === "run")) {
                    validate117.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema132.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs9 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.scope !== undefined) {
                    const _errs10 = errors;
                    if (!validate69(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData })) {
                      vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.record !== undefined) {
                      const _errs11 = errors;
                      if (!validate40(data.record, { instancePath: instancePath + "/record", parentData: data, parentDataProperty: "record", rootData })) {
                        vErrors = vErrors === null ? validate40.errors : vErrors.concat(validate40.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.contracts !== undefined) {
                        let data6 = data.contracts;
                        const _errs12 = errors;
                        if (errors === _errs12) {
                          if (Array.isArray(data6)) {
                            if (data6.length > 100) {
                              validate117.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                              return false;
                            } else {
                              var valid3 = true;
                              const len0 = data6.length;
                              for (let i0 = 0;i0 < len0; i0++) {
                                const _errs14 = errors;
                                if (!validate40(data6[i0], { instancePath: instancePath + "/contracts/" + i0, parentData: data6, parentDataProperty: i0, rootData })) {
                                  vErrors = vErrors === null ? validate40.errors : vErrors.concat(validate40.errors);
                                  errors = vErrors.length;
                                }
                                var valid3 = _errs14 === errors;
                                if (!valid3) {
                                  break;
                                }
                              }
                            }
                          } else {
                            validate117.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
    } else {
      validate117.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate117.errors = vErrors;
  return errors === 0;
}
var validateEffectiveContract = validate121;
var schema135 = { type: "object", properties: { schema: { const: "common.git-work-effective.v3" }, task: { $ref: "#/$defs/TaskRef" }, workRevision: { $ref: "#/$defs/Revision" }, taskRevision: { $ref: "#/$defs/Revision" }, workContracts: { $ref: "#/$defs/ContractSet" }, taskOverrides: { $ref: "#/$defs/ContractOverrides" }, contracts: { $ref: "#/$defs/ContractSet" }, parentTaskChain: { type: "array", items: { $ref: "#/$defs/Ancestor" }, maxItems: 2000 }, taskResources: { type: "array", items: { $ref: "#/$defs/ResourceClaim" }, maxItems: 2000 }, acceptanceCriteria: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 2000 }, dependencies: { type: "array", items: { $ref: "#/$defs/Dependency" }, maxItems: 1e4 }, dependencyTasks: { type: "array", items: { $ref: "#/$defs/DependencyTask" }, maxItems: 2000 }, digest: { $ref: "#/$defs/Digest" } }, required: ["schema", "task", "workRevision", "taskRevision", "workContracts", "taskOverrides", "contracts", "parentTaskChain", "taskResources", "acceptanceCriteria", "dependencies", "dependencyTasks", "digest"], additionalProperties: false };
function validate126(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts")) {
        validate126.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "revision" || key0 === "contracts")) {
            validate126.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate62(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.revision !== undefined) {
              let data1 = data.revision;
              const _errs3 = errors;
              const _errs4 = errors;
              if (!(typeof data1 == "number" && (!(data1 % 1) && !isNaN(data1)) && isFinite(data1))) {
                validate126.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                return false;
              }
              if (errors === _errs4) {
                if (typeof data1 == "number" && isFinite(data1)) {
                  if (data1 > 9007199254740991 || isNaN(data1)) {
                    validate126.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                    return false;
                  } else {
                    if (data1 < 1 || isNaN(data1)) {
                      validate126.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                      return false;
                    }
                  }
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.contracts !== undefined) {
                const _errs6 = errors;
                if (!validate17(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                  vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
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
    } else {
      validate126.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate126.errors = vErrors;
  return errors === 0;
}
function validate132(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.task === undefined && (missing0 = "task") || data.revision === undefined && (missing0 = "revision") || data.contracts === undefined && (missing0 = "contracts") || data.resources === undefined && (missing0 = "resources") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria")) {
        validate132.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "task" || key0 === "revision" || key0 === "contracts" || key0 === "resources" || key0 === "acceptanceCriteria")) {
            validate132.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.task !== undefined) {
            const _errs2 = errors;
            if (!validate62(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
              vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.revision !== undefined) {
              let data1 = data.revision;
              const _errs3 = errors;
              const _errs4 = errors;
              if (!(typeof data1 == "number" && (!(data1 % 1) && !isNaN(data1)) && isFinite(data1))) {
                validate132.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                return false;
              }
              if (errors === _errs4) {
                if (typeof data1 == "number" && isFinite(data1)) {
                  if (data1 > 9007199254740991 || isNaN(data1)) {
                    validate132.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                    return false;
                  } else {
                    if (data1 < 1 || isNaN(data1)) {
                      validate132.errors = [{ instancePath: instancePath + "/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                      return false;
                    }
                  }
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.contracts !== undefined) {
                const _errs6 = errors;
                if (!validate17(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                  vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.resources !== undefined) {
                  let data3 = data.resources;
                  const _errs7 = errors;
                  if (errors === _errs7) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 2000) {
                        validate132.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                        return false;
                      } else {
                        var valid2 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs9 = errors;
                          if (!validate32(data3[i0], { instancePath: instancePath + "/resources/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate32.errors : vErrors.concat(validate32.errors);
                            errors = vErrors.length;
                          }
                          var valid2 = _errs9 === errors;
                          if (!valid2) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate132.errors = [{ instancePath: instancePath + "/resources", schemaPath: "#/properties/resources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.acceptanceCriteria !== undefined) {
                    let data5 = data.acceptanceCriteria;
                    const _errs10 = errors;
                    if (errors === _errs10) {
                      if (Array.isArray(data5)) {
                        if (data5.length > 2000) {
                          validate132.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                          return false;
                        } else {
                          var valid3 = true;
                          const len1 = data5.length;
                          for (let i1 = 0;i1 < len1; i1++) {
                            let data6 = data5[i1];
                            const _errs12 = errors;
                            const _errs13 = errors;
                            if (errors === _errs13) {
                              if (typeof data6 === "string") {
                                if (func3(data6) > 240) {
                                  validate132.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i1, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                  return false;
                                } else {
                                  if (func3(data6) < 1) {
                                    validate132.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i1, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                    return false;
                                  } else {
                                    if (!pattern0.test(data6)) {
                                      validate132.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i1, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                      return false;
                                    }
                                  }
                                }
                              } else {
                                validate132.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i1, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                            }
                            var valid3 = _errs12 === errors;
                            if (!valid3) {
                              break;
                            }
                          }
                        }
                      } else {
                        validate132.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
    } else {
      validate132.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate132.errors = vErrors;
  return errors === 0;
}
function validate121(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.taskOverrides === undefined && (missing0 = "taskOverrides") || data.contracts === undefined && (missing0 = "contracts") || data.parentTaskChain === undefined && (missing0 = "parentTaskChain") || data.taskResources === undefined && (missing0 = "taskResources") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.dependencies === undefined && (missing0 = "dependencies") || data.dependencyTasks === undefined && (missing0 = "dependencyTasks") || data.digest === undefined && (missing0 = "digest")) {
        validate121.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema135.properties, key0)) {
            validate121.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-effective.v3") {
              validate121.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-effective.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs3 = errors;
              if (!validate62(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.workRevision !== undefined) {
                let data2 = data.workRevision;
                const _errs4 = errors;
                const _errs5 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  validate121.errors = [{ instancePath: instancePath + "/workRevision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                  return false;
                }
                if (errors === _errs5) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      validate121.errors = [{ instancePath: instancePath + "/workRevision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                      return false;
                    } else {
                      if (data2 < 1 || isNaN(data2)) {
                        validate121.errors = [{ instancePath: instancePath + "/workRevision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                        return false;
                      }
                    }
                  }
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.taskRevision !== undefined) {
                  let data3 = data.taskRevision;
                  const _errs7 = errors;
                  const _errs8 = errors;
                  if (!(typeof data3 == "number" && (!(data3 % 1) && !isNaN(data3)) && isFinite(data3))) {
                    validate121.errors = [{ instancePath: instancePath + "/taskRevision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                    return false;
                  }
                  if (errors === _errs8) {
                    if (typeof data3 == "number" && isFinite(data3)) {
                      if (data3 > 9007199254740991 || isNaN(data3)) {
                        validate121.errors = [{ instancePath: instancePath + "/taskRevision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                        return false;
                      } else {
                        if (data3 < 1 || isNaN(data3)) {
                          validate121.errors = [{ instancePath: instancePath + "/taskRevision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                          return false;
                        }
                      }
                    }
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.workContracts !== undefined) {
                    const _errs10 = errors;
                    if (!validate17(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                      vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.taskOverrides !== undefined) {
                      const _errs11 = errors;
                      if (!validate27(data.taskOverrides, { instancePath: instancePath + "/taskOverrides", parentData: data, parentDataProperty: "taskOverrides", rootData })) {
                        vErrors = vErrors === null ? validate27.errors : vErrors.concat(validate27.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.contracts !== undefined) {
                        const _errs12 = errors;
                        if (!validate17(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                          vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.parentTaskChain !== undefined) {
                          let data7 = data.parentTaskChain;
                          const _errs13 = errors;
                          if (errors === _errs13) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate121.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid3 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs15 = errors;
                                  if (!validate126(data7[i0], { instancePath: instancePath + "/parentTaskChain/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate126.errors : vErrors.concat(validate126.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid3 = _errs15 === errors;
                                  if (!valid3) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate121.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs13 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.taskResources !== undefined) {
                            let data9 = data.taskResources;
                            const _errs16 = errors;
                            if (errors === _errs16) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate121.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid4 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs18 = errors;
                                    if (!validate32(data9[i1], { instancePath: instancePath + "/taskResources/" + i1, parentData: data9, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate32.errors : vErrors.concat(validate32.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid4 = _errs18 === errors;
                                    if (!valid4) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate121.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs16 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.acceptanceCriteria !== undefined) {
                              let data11 = data.acceptanceCriteria;
                              const _errs19 = errors;
                              if (errors === _errs19) {
                                if (Array.isArray(data11)) {
                                  if (data11.length > 2000) {
                                    validate121.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid5 = true;
                                    const len2 = data11.length;
                                    for (let i2 = 0;i2 < len2; i2++) {
                                      let data12 = data11[i2];
                                      const _errs21 = errors;
                                      const _errs22 = errors;
                                      if (errors === _errs22) {
                                        if (typeof data12 === "string") {
                                          if (func3(data12) > 240) {
                                            validate121.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                            return false;
                                          } else {
                                            if (func3(data12) < 1) {
                                              validate121.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                              return false;
                                            } else {
                                              if (!pattern0.test(data12)) {
                                                validate121.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                                return false;
                                              }
                                            }
                                          }
                                        } else {
                                          validate121.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                      }
                                      var valid5 = _errs21 === errors;
                                      if (!valid5) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate121.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs19 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.dependencies !== undefined) {
                                let data13 = data.dependencies;
                                const _errs24 = errors;
                                if (errors === _errs24) {
                                  if (Array.isArray(data13)) {
                                    if (data13.length > 1e4) {
                                      validate121.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                      return false;
                                    } else {
                                      var valid7 = true;
                                      const len3 = data13.length;
                                      for (let i3 = 0;i3 < len3; i3++) {
                                        const _errs26 = errors;
                                        if (!validate61(data13[i3], { instancePath: instancePath + "/dependencies/" + i3, parentData: data13, parentDataProperty: i3, rootData })) {
                                          vErrors = vErrors === null ? validate61.errors : vErrors.concat(validate61.errors);
                                          errors = vErrors.length;
                                        }
                                        var valid7 = _errs26 === errors;
                                        if (!valid7) {
                                          break;
                                        }
                                      }
                                    }
                                  } else {
                                    validate121.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs24 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.dependencyTasks !== undefined) {
                                  let data15 = data.dependencyTasks;
                                  const _errs27 = errors;
                                  if (errors === _errs27) {
                                    if (Array.isArray(data15)) {
                                      if (data15.length > 2000) {
                                        validate121.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                        return false;
                                      } else {
                                        var valid8 = true;
                                        const len4 = data15.length;
                                        for (let i4 = 0;i4 < len4; i4++) {
                                          const _errs29 = errors;
                                          if (!validate132(data15[i4], { instancePath: instancePath + "/dependencyTasks/" + i4, parentData: data15, parentDataProperty: i4, rootData })) {
                                            vErrors = vErrors === null ? validate132.errors : vErrors.concat(validate132.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid8 = _errs29 === errors;
                                          if (!valid8) {
                                            break;
                                          }
                                        }
                                      }
                                    } else {
                                      validate121.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs27 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.digest !== undefined) {
                                    let data17 = data.digest;
                                    const _errs30 = errors;
                                    const _errs31 = errors;
                                    if (errors === _errs31) {
                                      if (typeof data17 === "string") {
                                        if (!pattern16.test(data17)) {
                                          validate121.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                                          return false;
                                        }
                                      } else {
                                        validate121.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid0 = _errs30 === errors;
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
      validate121.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate121.errors = vErrors;
  return errors === 0;
}
var validateAllocationBinding = validate137;
var schema145 = { type: "object", properties: { schema: { const: "common.git-work-allocation.v3" }, ownerId: { $ref: "#/$defs/Id" }, createRequestId: { $ref: "#/$defs/Id" }, requestDigest: { $ref: "#/$defs/Digest" }, kind: { enum: ["work", "task", "run"] }, scope: { $ref: "#/$defs/Scope" }, id: { type: "string", pattern: "^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$" }, recordPath: { type: "string", maxLength: 4096 }, recordSha256: { $ref: "#/$defs/Digest" }, contracts: { type: "array", maxItems: 100, items: { type: "object", properties: { path: { type: "string", maxLength: 4096 }, sha256: { $ref: "#/$defs/Digest" } }, required: ["path", "sha256"], additionalProperties: false } }, authority: { type: "object", properties: { actorId: { $ref: "#/$defs/Id" }, revision: { $ref: "#/$defs/Revision" }, authorityRef: { $ref: "#/$defs/Id" } }, required: ["actorId", "revision", "authorityRef"], additionalProperties: false } }, required: ["schema", "ownerId", "createRequestId", "requestDigest", "kind", "scope", "id", "recordPath", "recordSha256", "contracts", "authority"], additionalProperties: false };
var pattern84 = new RegExp("^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$", "u");
function validate137(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.ownerId === undefined && (missing0 = "ownerId") || data.createRequestId === undefined && (missing0 = "createRequestId") || data.requestDigest === undefined && (missing0 = "requestDigest") || data.kind === undefined && (missing0 = "kind") || data.scope === undefined && (missing0 = "scope") || data.id === undefined && (missing0 = "id") || data.recordPath === undefined && (missing0 = "recordPath") || data.recordSha256 === undefined && (missing0 = "recordSha256") || data.contracts === undefined && (missing0 = "contracts") || data.authority === undefined && (missing0 = "authority")) {
        validate137.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema145.properties, key0)) {
            validate137.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-allocation.v3") {
              validate137.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-allocation.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.ownerId !== undefined) {
              let data1 = data.ownerId;
              const _errs3 = errors;
              const _errs4 = errors;
              if (errors === _errs4) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 240) {
                    validate137.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate137.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern0.test(data1)) {
                        validate137.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate137.errors = [{ instancePath: instancePath + "/ownerId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.createRequestId !== undefined) {
                let data2 = data.createRequestId;
                const _errs6 = errors;
                const _errs7 = errors;
                if (errors === _errs7) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 240) {
                      validate137.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate137.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern0.test(data2)) {
                          validate137.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate137.errors = [{ instancePath: instancePath + "/createRequestId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.requestDigest !== undefined) {
                  let data3 = data.requestDigest;
                  const _errs9 = errors;
                  const _errs10 = errors;
                  if (errors === _errs10) {
                    if (typeof data3 === "string") {
                      if (!pattern16.test(data3)) {
                        validate137.errors = [{ instancePath: instancePath + "/requestDigest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                        return false;
                      }
                    } else {
                      validate137.errors = [{ instancePath: instancePath + "/requestDigest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs9 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.kind !== undefined) {
                    let data4 = data.kind;
                    const _errs12 = errors;
                    if (!(data4 === "work" || data4 === "task" || data4 === "run")) {
                      validate137.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema145.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid0 = _errs12 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.scope !== undefined) {
                      const _errs13 = errors;
                      if (!validate69(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData })) {
                        vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs13 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.id !== undefined) {
                        let data6 = data.id;
                        const _errs14 = errors;
                        if (errors === _errs14) {
                          if (typeof data6 === "string") {
                            if (!pattern84.test(data6)) {
                              validate137.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/properties/id/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$" }, message: 'must match pattern "' + "^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$" + '"' }];
                              return false;
                            }
                          } else {
                            validate137.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/properties/id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                        }
                        var valid0 = _errs14 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.recordPath !== undefined) {
                          let data7 = data.recordPath;
                          const _errs16 = errors;
                          if (errors === _errs16) {
                            if (typeof data7 === "string") {
                              if (func3(data7) > 4096) {
                                validate137.errors = [{ instancePath: instancePath + "/recordPath", schemaPath: "#/properties/recordPath/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                return false;
                              }
                            } else {
                              validate137.errors = [{ instancePath: instancePath + "/recordPath", schemaPath: "#/properties/recordPath/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                              return false;
                            }
                          }
                          var valid0 = _errs16 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.recordSha256 !== undefined) {
                            let data8 = data.recordSha256;
                            const _errs18 = errors;
                            const _errs19 = errors;
                            if (errors === _errs19) {
                              if (typeof data8 === "string") {
                                if (!pattern16.test(data8)) {
                                  validate137.errors = [{ instancePath: instancePath + "/recordSha256", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                                  return false;
                                }
                              } else {
                                validate137.errors = [{ instancePath: instancePath + "/recordSha256", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                            }
                            var valid0 = _errs18 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.contracts !== undefined) {
                              let data9 = data.contracts;
                              const _errs21 = errors;
                              if (errors === _errs21) {
                                if (Array.isArray(data9)) {
                                  if (data9.length > 100) {
                                    validate137.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                    return false;
                                  } else {
                                    var valid5 = true;
                                    const len0 = data9.length;
                                    for (let i0 = 0;i0 < len0; i0++) {
                                      let data10 = data9[i0];
                                      const _errs23 = errors;
                                      if (errors === _errs23) {
                                        if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                                          let missing1;
                                          if (data10.path === undefined && (missing1 = "path") || data10.sha256 === undefined && (missing1 = "sha256")) {
                                            validate137.errors = [{ instancePath: instancePath + "/contracts/" + i0, schemaPath: "#/properties/contracts/items/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                                            return false;
                                          } else {
                                            const _errs25 = errors;
                                            for (const key1 in data10) {
                                              if (!(key1 === "path" || key1 === "sha256")) {
                                                validate137.errors = [{ instancePath: instancePath + "/contracts/" + i0, schemaPath: "#/properties/contracts/items/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                                return false;
                                                break;
                                              }
                                            }
                                            if (_errs25 === errors) {
                                              if (data10.path !== undefined) {
                                                let data11 = data10.path;
                                                const _errs26 = errors;
                                                if (errors === _errs26) {
                                                  if (typeof data11 === "string") {
                                                    if (func3(data11) > 4096) {
                                                      validate137.errors = [{ instancePath: instancePath + "/contracts/" + i0 + "/path", schemaPath: "#/properties/contracts/items/properties/path/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                                                      return false;
                                                    }
                                                  } else {
                                                    validate137.errors = [{ instancePath: instancePath + "/contracts/" + i0 + "/path", schemaPath: "#/properties/contracts/items/properties/path/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                    return false;
                                                  }
                                                }
                                                var valid6 = _errs26 === errors;
                                              } else {
                                                var valid6 = true;
                                              }
                                              if (valid6) {
                                                if (data10.sha256 !== undefined) {
                                                  let data12 = data10.sha256;
                                                  const _errs28 = errors;
                                                  const _errs29 = errors;
                                                  if (errors === _errs29) {
                                                    if (typeof data12 === "string") {
                                                      if (!pattern16.test(data12)) {
                                                        validate137.errors = [{ instancePath: instancePath + "/contracts/" + i0 + "/sha256", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                                                        return false;
                                                      }
                                                    } else {
                                                      validate137.errors = [{ instancePath: instancePath + "/contracts/" + i0 + "/sha256", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                      return false;
                                                    }
                                                  }
                                                  var valid6 = _errs28 === errors;
                                                } else {
                                                  var valid6 = true;
                                                }
                                              }
                                            }
                                          }
                                        } else {
                                          validate137.errors = [{ instancePath: instancePath + "/contracts/" + i0, schemaPath: "#/properties/contracts/items/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                          return false;
                                        }
                                      }
                                      var valid5 = _errs23 === errors;
                                      if (!valid5) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate137.errors = [{ instancePath: instancePath + "/contracts", schemaPath: "#/properties/contracts/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs21 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.authority !== undefined) {
                                let data13 = data.authority;
                                const _errs31 = errors;
                                if (errors === _errs31) {
                                  if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                                    let missing2;
                                    if (data13.actorId === undefined && (missing2 = "actorId") || data13.revision === undefined && (missing2 = "revision") || data13.authorityRef === undefined && (missing2 = "authorityRef")) {
                                      validate137.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                                      return false;
                                    } else {
                                      const _errs33 = errors;
                                      for (const key2 in data13) {
                                        if (!(key2 === "actorId" || key2 === "revision" || key2 === "authorityRef")) {
                                          validate137.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                          return false;
                                          break;
                                        }
                                      }
                                      if (_errs33 === errors) {
                                        if (data13.actorId !== undefined) {
                                          let data14 = data13.actorId;
                                          const _errs34 = errors;
                                          const _errs35 = errors;
                                          if (errors === _errs35) {
                                            if (typeof data14 === "string") {
                                              if (func3(data14) > 240) {
                                                validate137.errors = [{ instancePath: instancePath + "/authority/actorId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                                return false;
                                              } else {
                                                if (func3(data14) < 1) {
                                                  validate137.errors = [{ instancePath: instancePath + "/authority/actorId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                                  return false;
                                                } else {
                                                  if (!pattern0.test(data14)) {
                                                    validate137.errors = [{ instancePath: instancePath + "/authority/actorId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                                    return false;
                                                  }
                                                }
                                              }
                                            } else {
                                              validate137.errors = [{ instancePath: instancePath + "/authority/actorId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                              return false;
                                            }
                                          }
                                          var valid8 = _errs34 === errors;
                                        } else {
                                          var valid8 = true;
                                        }
                                        if (valid8) {
                                          if (data13.revision !== undefined) {
                                            let data15 = data13.revision;
                                            const _errs37 = errors;
                                            const _errs38 = errors;
                                            if (!(typeof data15 == "number" && (!(data15 % 1) && !isNaN(data15)) && isFinite(data15))) {
                                              validate137.errors = [{ instancePath: instancePath + "/authority/revision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                              return false;
                                            }
                                            if (errors === _errs38) {
                                              if (typeof data15 == "number" && isFinite(data15)) {
                                                if (data15 > 9007199254740991 || isNaN(data15)) {
                                                  validate137.errors = [{ instancePath: instancePath + "/authority/revision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                                                  return false;
                                                } else {
                                                  if (data15 < 1 || isNaN(data15)) {
                                                    validate137.errors = [{ instancePath: instancePath + "/authority/revision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                                    return false;
                                                  }
                                                }
                                              }
                                            }
                                            var valid8 = _errs37 === errors;
                                          } else {
                                            var valid8 = true;
                                          }
                                          if (valid8) {
                                            if (data13.authorityRef !== undefined) {
                                              let data16 = data13.authorityRef;
                                              const _errs40 = errors;
                                              const _errs41 = errors;
                                              if (errors === _errs41) {
                                                if (typeof data16 === "string") {
                                                  if (func3(data16) > 240) {
                                                    validate137.errors = [{ instancePath: instancePath + "/authority/authorityRef", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                                    return false;
                                                  } else {
                                                    if (func3(data16) < 1) {
                                                      validate137.errors = [{ instancePath: instancePath + "/authority/authorityRef", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                                      return false;
                                                    } else {
                                                      if (!pattern0.test(data16)) {
                                                        validate137.errors = [{ instancePath: instancePath + "/authority/authorityRef", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                                        return false;
                                                      }
                                                    }
                                                  }
                                                } else {
                                                  validate137.errors = [{ instancePath: instancePath + "/authority/authorityRef", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                  return false;
                                                }
                                              }
                                              var valid8 = _errs40 === errors;
                                            } else {
                                              var valid8 = true;
                                            }
                                          }
                                        }
                                      }
                                    }
                                  } else {
                                    validate137.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
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
    } else {
      validate137.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate137.errors = vErrors;
  return errors === 0;
}
var validateStoredRecord = validate139;
function validate142(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.runId === undefined && (missing0 = "runId") || data.task === undefined && (missing0 = "task") || data.attempt === undefined && (missing0 = "attempt") || data.status === undefined && (missing0 = "status") || data.effectiveContractDigest === undefined && (missing0 = "effectiveContractDigest") || data.sourceSnapshotId === undefined && (missing0 = "sourceSnapshotId") || data.parentAuthorityRef === undefined && (missing0 = "parentAuthorityRef") || data.nativeBindings === undefined && (missing0 = "nativeBindings") || data.effectRefs === undefined && (missing0 = "effectRefs")) {
        validate142.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema115.properties, key0)) {
            validate142.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.runId !== undefined) {
            let data0 = data.runId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (func3(data0) > 240) {
                  validate142.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                  return false;
                } else {
                  if (func3(data0) < 1) {
                    validate142.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                    return false;
                  } else {
                    if (!pattern0.test(data0)) {
                      validate142.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                      return false;
                    }
                  }
                }
              } else {
                validate142.errors = [{ instancePath: instancePath + "/runId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs5 = errors;
              if (!validate62(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.attempt !== undefined) {
                let data2 = data.attempt;
                const _errs6 = errors;
                const _errs7 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  validate142.errors = [{ instancePath: instancePath + "/attempt", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                  return false;
                }
                if (errors === _errs7) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      validate142.errors = [{ instancePath: instancePath + "/attempt", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                      return false;
                    } else {
                      if (data2 < 1 || isNaN(data2)) {
                        validate142.errors = [{ instancePath: instancePath + "/attempt", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                        return false;
                      }
                    }
                  }
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.status !== undefined) {
                  let data3 = data.status;
                  const _errs9 = errors;
                  if (!(data3 === "reserved" || data3 === "started" || data3 === "succeeded" || data3 === "failed" || data3 === "cancelled" || data3 === "uncertain")) {
                    validate142.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema115.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs9 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.effectiveContractDigest !== undefined) {
                    let data4 = data.effectiveContractDigest;
                    const _errs10 = errors;
                    const _errs11 = errors;
                    if (errors === _errs11) {
                      if (typeof data4 === "string") {
                        if (!pattern16.test(data4)) {
                          validate142.errors = [{ instancePath: instancePath + "/effectiveContractDigest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                          return false;
                        }
                      } else {
                        validate142.errors = [{ instancePath: instancePath + "/effectiveContractDigest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.sourceSnapshotId !== undefined) {
                      let data5 = data.sourceSnapshotId;
                      const _errs13 = errors;
                      const _errs14 = errors;
                      if (errors === _errs14) {
                        if (typeof data5 === "string") {
                          if (func3(data5) > 240) {
                            validate142.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                            return false;
                          } else {
                            if (func3(data5) < 1) {
                              validate142.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            } else {
                              if (!pattern0.test(data5)) {
                                validate142.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                return false;
                              }
                            }
                          }
                        } else {
                          validate142.errors = [{ instancePath: instancePath + "/sourceSnapshotId", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid0 = _errs13 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.parentAuthorityRef !== undefined) {
                        let data6 = data.parentAuthorityRef;
                        const _errs16 = errors;
                        const _errs17 = errors;
                        if (errors === _errs17) {
                          if (typeof data6 === "string") {
                            if (func3(data6) > 240) {
                              validate142.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                              return false;
                            } else {
                              if (func3(data6) < 1) {
                                validate142.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                return false;
                              } else {
                                if (!pattern0.test(data6)) {
                                  validate142.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                  return false;
                                }
                              }
                            }
                          } else {
                            validate142.errors = [{ instancePath: instancePath + "/parentAuthorityRef", schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                        }
                        var valid0 = _errs16 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.nativeBindings !== undefined) {
                          let data7 = data.nativeBindings;
                          const _errs19 = errors;
                          if (errors === _errs19) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate142.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid6 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs21 = errors;
                                  if (!validate84(data7[i0], { instancePath: instancePath + "/nativeBindings/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate84.errors : vErrors.concat(validate84.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid6 = _errs21 === errors;
                                  if (!valid6) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate142.errors = [{ instancePath: instancePath + "/nativeBindings", schemaPath: "#/properties/nativeBindings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs19 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.effectRefs !== undefined) {
                            let data9 = data.effectRefs;
                            const _errs22 = errors;
                            if (errors === _errs22) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate142.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid7 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    let data10 = data9[i1];
                                    const _errs24 = errors;
                                    const _errs25 = errors;
                                    if (errors === _errs25) {
                                      if (typeof data10 === "string") {
                                        if (func3(data10) > 240) {
                                          validate142.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                          return false;
                                        } else {
                                          if (func3(data10) < 1) {
                                            validate142.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                            return false;
                                          } else {
                                            if (!pattern0.test(data10)) {
                                              validate142.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                              return false;
                                            }
                                          }
                                        }
                                      } else {
                                        validate142.errors = [{ instancePath: instancePath + "/effectRefs/" + i1, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid7 = _errs24 === errors;
                                    if (!valid7) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate142.errors = [{ instancePath: instancePath + "/effectRefs", schemaPath: "#/properties/effectRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs22 === errors;
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
      validate142.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate142.errors = vErrors;
  return errors === 0;
}
function validate139(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
                if (!validate16(data.value, { instancePath: instancePath + "/value", parentData: data, parentDataProperty: "value", rootData })) {
                  vErrors = vErrors === null ? validate16.errors : vErrors.concat(validate16.errors);
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
                if (!validate26(data.value, { instancePath: instancePath + "/value", parentData: data, parentDataProperty: "value", rootData })) {
                  vErrors = vErrors === null ? validate26.errors : vErrors.concat(validate26.errors);
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
                  if (!validate142(data.value, { instancePath: instancePath + "/value", parentData: data, parentDataProperty: "value", rootData })) {
                    vErrors = vErrors === null ? validate142.errors : vErrors.concat(validate142.errors);
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
    validate139.errors = vErrors;
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
  validate139.errors = vErrors;
  return errors === 0;
}
var validateContextPacket = validate146;
var schema162 = { type: "object", properties: { schema: { const: "common.git-work-context.v3" }, mode: { enum: ["full", "compact", "dispatch", "resume"] }, effective: { $ref: "#/$defs/EffectiveContract" }, contractSources: { type: "array", maxItems: 1e4, items: { $ref: "#/$defs/Contract" } }, authority: { const: "none" }, nativePermissionGranted: { const: false }, semanticAcceptanceEstablished: { const: false } }, required: ["schema", "mode", "effective", "contractSources", "authority", "nativePermissionGranted", "semanticAcceptanceEstablished"], additionalProperties: false };
function validate147(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.task === undefined && (missing0 = "task") || data.workRevision === undefined && (missing0 = "workRevision") || data.taskRevision === undefined && (missing0 = "taskRevision") || data.workContracts === undefined && (missing0 = "workContracts") || data.taskOverrides === undefined && (missing0 = "taskOverrides") || data.contracts === undefined && (missing0 = "contracts") || data.parentTaskChain === undefined && (missing0 = "parentTaskChain") || data.taskResources === undefined && (missing0 = "taskResources") || data.acceptanceCriteria === undefined && (missing0 = "acceptanceCriteria") || data.dependencies === undefined && (missing0 = "dependencies") || data.dependencyTasks === undefined && (missing0 = "dependencyTasks") || data.digest === undefined && (missing0 = "digest")) {
        validate147.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema135.properties, key0)) {
            validate147.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-effective.v3") {
              validate147.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-effective.v3" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.task !== undefined) {
              const _errs3 = errors;
              if (!validate62(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.workRevision !== undefined) {
                let data2 = data.workRevision;
                const _errs4 = errors;
                const _errs5 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  validate147.errors = [{ instancePath: instancePath + "/workRevision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                  return false;
                }
                if (errors === _errs5) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      validate147.errors = [{ instancePath: instancePath + "/workRevision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                      return false;
                    } else {
                      if (data2 < 1 || isNaN(data2)) {
                        validate147.errors = [{ instancePath: instancePath + "/workRevision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                        return false;
                      }
                    }
                  }
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.taskRevision !== undefined) {
                  let data3 = data.taskRevision;
                  const _errs7 = errors;
                  const _errs8 = errors;
                  if (!(typeof data3 == "number" && (!(data3 % 1) && !isNaN(data3)) && isFinite(data3))) {
                    validate147.errors = [{ instancePath: instancePath + "/taskRevision", schemaPath: "#/$defs/Revision/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                    return false;
                  }
                  if (errors === _errs8) {
                    if (typeof data3 == "number" && isFinite(data3)) {
                      if (data3 > 9007199254740991 || isNaN(data3)) {
                        validate147.errors = [{ instancePath: instancePath + "/taskRevision", schemaPath: "#/$defs/Revision/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                        return false;
                      } else {
                        if (data3 < 1 || isNaN(data3)) {
                          validate147.errors = [{ instancePath: instancePath + "/taskRevision", schemaPath: "#/$defs/Revision/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                          return false;
                        }
                      }
                    }
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.workContracts !== undefined) {
                    const _errs10 = errors;
                    if (!validate17(data.workContracts, { instancePath: instancePath + "/workContracts", parentData: data, parentDataProperty: "workContracts", rootData })) {
                      vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.taskOverrides !== undefined) {
                      const _errs11 = errors;
                      if (!validate27(data.taskOverrides, { instancePath: instancePath + "/taskOverrides", parentData: data, parentDataProperty: "taskOverrides", rootData })) {
                        vErrors = vErrors === null ? validate27.errors : vErrors.concat(validate27.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.contracts !== undefined) {
                        const _errs12 = errors;
                        if (!validate17(data.contracts, { instancePath: instancePath + "/contracts", parentData: data, parentDataProperty: "contracts", rootData })) {
                          vErrors = vErrors === null ? validate17.errors : vErrors.concat(validate17.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.parentTaskChain !== undefined) {
                          let data7 = data.parentTaskChain;
                          const _errs13 = errors;
                          if (errors === _errs13) {
                            if (Array.isArray(data7)) {
                              if (data7.length > 2000) {
                                validate147.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                return false;
                              } else {
                                var valid3 = true;
                                const len0 = data7.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs15 = errors;
                                  if (!validate126(data7[i0], { instancePath: instancePath + "/parentTaskChain/" + i0, parentData: data7, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate126.errors : vErrors.concat(validate126.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid3 = _errs15 === errors;
                                  if (!valid3) {
                                    break;
                                  }
                                }
                              }
                            } else {
                              validate147.errors = [{ instancePath: instancePath + "/parentTaskChain", schemaPath: "#/properties/parentTaskChain/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs13 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.taskResources !== undefined) {
                            let data9 = data.taskResources;
                            const _errs16 = errors;
                            if (errors === _errs16) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 2000) {
                                  validate147.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                  return false;
                                } else {
                                  var valid4 = true;
                                  const len1 = data9.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs18 = errors;
                                    if (!validate32(data9[i1], { instancePath: instancePath + "/taskResources/" + i1, parentData: data9, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate32.errors : vErrors.concat(validate32.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid4 = _errs18 === errors;
                                    if (!valid4) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate147.errors = [{ instancePath: instancePath + "/taskResources", schemaPath: "#/properties/taskResources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid0 = _errs16 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.acceptanceCriteria !== undefined) {
                              let data11 = data.acceptanceCriteria;
                              const _errs19 = errors;
                              if (errors === _errs19) {
                                if (Array.isArray(data11)) {
                                  if (data11.length > 2000) {
                                    validate147.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                    return false;
                                  } else {
                                    var valid5 = true;
                                    const len2 = data11.length;
                                    for (let i2 = 0;i2 < len2; i2++) {
                                      let data12 = data11[i2];
                                      const _errs21 = errors;
                                      const _errs22 = errors;
                                      if (errors === _errs22) {
                                        if (typeof data12 === "string") {
                                          if (func3(data12) > 240) {
                                            validate147.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/maxLength", keyword: "maxLength", params: { limit: 240 }, message: "must NOT have more than 240 characters" }];
                                            return false;
                                          } else {
                                            if (func3(data12) < 1) {
                                              validate147.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                              return false;
                                            } else {
                                              if (!pattern0.test(data12)) {
                                                validate147.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/pattern", keyword: "pattern", params: { pattern: "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" }, message: 'must match pattern "' + "^(?!\\s)(?!.*\\s$)[^\\u0000-\\u001f\\u007f]+$" + '"' }];
                                                return false;
                                              }
                                            }
                                          }
                                        } else {
                                          validate147.errors = [{ instancePath: instancePath + "/acceptanceCriteria/" + i2, schemaPath: "#/$defs/Id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                      }
                                      var valid5 = _errs21 === errors;
                                      if (!valid5) {
                                        break;
                                      }
                                    }
                                  }
                                } else {
                                  validate147.errors = [{ instancePath: instancePath + "/acceptanceCriteria", schemaPath: "#/properties/acceptanceCriteria/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs19 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.dependencies !== undefined) {
                                let data13 = data.dependencies;
                                const _errs24 = errors;
                                if (errors === _errs24) {
                                  if (Array.isArray(data13)) {
                                    if (data13.length > 1e4) {
                                      validate147.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                                      return false;
                                    } else {
                                      var valid7 = true;
                                      const len3 = data13.length;
                                      for (let i3 = 0;i3 < len3; i3++) {
                                        const _errs26 = errors;
                                        if (!validate61(data13[i3], { instancePath: instancePath + "/dependencies/" + i3, parentData: data13, parentDataProperty: i3, rootData })) {
                                          vErrors = vErrors === null ? validate61.errors : vErrors.concat(validate61.errors);
                                          errors = vErrors.length;
                                        }
                                        var valid7 = _errs26 === errors;
                                        if (!valid7) {
                                          break;
                                        }
                                      }
                                    }
                                  } else {
                                    validate147.errors = [{ instancePath: instancePath + "/dependencies", schemaPath: "#/properties/dependencies/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs24 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.dependencyTasks !== undefined) {
                                  let data15 = data.dependencyTasks;
                                  const _errs27 = errors;
                                  if (errors === _errs27) {
                                    if (Array.isArray(data15)) {
                                      if (data15.length > 2000) {
                                        validate147.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/maxItems", keyword: "maxItems", params: { limit: 2000 }, message: "must NOT have more than 2000 items" }];
                                        return false;
                                      } else {
                                        var valid8 = true;
                                        const len4 = data15.length;
                                        for (let i4 = 0;i4 < len4; i4++) {
                                          const _errs29 = errors;
                                          if (!validate132(data15[i4], { instancePath: instancePath + "/dependencyTasks/" + i4, parentData: data15, parentDataProperty: i4, rootData })) {
                                            vErrors = vErrors === null ? validate132.errors : vErrors.concat(validate132.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid8 = _errs29 === errors;
                                          if (!valid8) {
                                            break;
                                          }
                                        }
                                      }
                                    } else {
                                      validate147.errors = [{ instancePath: instancePath + "/dependencyTasks", schemaPath: "#/properties/dependencyTasks/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs27 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.digest !== undefined) {
                                    let data17 = data.digest;
                                    const _errs30 = errors;
                                    const _errs31 = errors;
                                    if (errors === _errs31) {
                                      if (typeof data17 === "string") {
                                        if (!pattern16.test(data17)) {
                                          validate147.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                                          return false;
                                        }
                                      } else {
                                        validate147.errors = [{ instancePath: instancePath + "/digest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid0 = _errs30 === errors;
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
      validate147.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate147.errors = vErrors;
  return errors === 0;
}
function validate146(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.mode === undefined && (missing0 = "mode") || data.effective === undefined && (missing0 = "effective") || data.contractSources === undefined && (missing0 = "contractSources") || data.authority === undefined && (missing0 = "authority") || data.nativePermissionGranted === undefined && (missing0 = "nativePermissionGranted") || data.semanticAcceptanceEstablished === undefined && (missing0 = "semanticAcceptanceEstablished")) {
        validate146.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "mode" || key0 === "effective" || key0 === "contractSources" || key0 === "authority" || key0 === "nativePermissionGranted" || key0 === "semanticAcceptanceEstablished")) {
            validate146.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-context.v3") {
              validate146.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-context.v3" }, message: "must be equal to constant" }];
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
                validate146.errors = [{ instancePath: instancePath + "/mode", schemaPath: "#/properties/mode/enum", keyword: "enum", params: { allowedValues: schema162.properties.mode.enum }, message: "must be equal to one of the allowed values" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.effective !== undefined) {
                const _errs4 = errors;
                if (!validate147(data.effective, { instancePath: instancePath + "/effective", parentData: data, parentDataProperty: "effective", rootData })) {
                  vErrors = vErrors === null ? validate147.errors : vErrors.concat(validate147.errors);
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
                        validate146.errors = [{ instancePath: instancePath + "/contractSources", schemaPath: "#/properties/contractSources/maxItems", keyword: "maxItems", params: { limit: 1e4 }, message: "must NOT have more than 10000 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate35(data3[i0], { instancePath: instancePath + "/contractSources/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate35.errors : vErrors.concat(validate35.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate146.errors = [{ instancePath: instancePath + "/contractSources", schemaPath: "#/properties/contractSources/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
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
                      validate146.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/const", keyword: "const", params: { allowedValue: "none" }, message: "must be equal to constant" }];
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
                        validate146.errors = [{ instancePath: instancePath + "/nativePermissionGranted", schemaPath: "#/properties/nativePermissionGranted/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
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
                          validate146.errors = [{ instancePath: instancePath + "/semanticAcceptanceEstablished", schemaPath: "#/properties/semanticAcceptanceEstablished/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
                          return false;
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
      validate146.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate146.errors = vErrors;
  return errors === 0;
}
var validateCompactContextPacket = validate158;
function validate158(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.encoding === undefined && (missing0 = "encoding") || data.effectiveDigest === undefined && (missing0 = "effectiveDigest") || data.decodedBytes === undefined && (missing0 = "decodedBytes") || data.payload === undefined && (missing0 = "payload") || data.authority === undefined && (missing0 = "authority") || data.nativePermissionGranted === undefined && (missing0 = "nativePermissionGranted") || data.semanticAcceptanceEstablished === undefined && (missing0 = "semanticAcceptanceEstablished")) {
        validate158.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "encoding" || key0 === "effectiveDigest" || key0 === "decodedBytes" || key0 === "payload" || key0 === "authority" || key0 === "nativePermissionGranted" || key0 === "semanticAcceptanceEstablished")) {
            validate158.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.git-work-context.compact.v3") {
              validate158.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.git-work-context.compact.v3" }, message: "must be equal to constant" }];
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
                validate158.errors = [{ instancePath: instancePath + "/encoding", schemaPath: "#/properties/encoding/const", keyword: "const", params: { allowedValue: "deflate-raw+base64.v1" }, message: "must be equal to constant" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.effectiveDigest !== undefined) {
                let data2 = data.effectiveDigest;
                const _errs4 = errors;
                const _errs5 = errors;
                if (errors === _errs5) {
                  if (typeof data2 === "string") {
                    if (!pattern16.test(data2)) {
                      validate158.errors = [{ instancePath: instancePath + "/effectiveDigest", schemaPath: "#/$defs/Digest/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
                      return false;
                    }
                  } else {
                    validate158.errors = [{ instancePath: instancePath + "/effectiveDigest", schemaPath: "#/$defs/Digest/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.decodedBytes !== undefined) {
                  let data3 = data.decodedBytes;
                  const _errs7 = errors;
                  if (!(typeof data3 == "number" && (!(data3 % 1) && !isNaN(data3)) && isFinite(data3))) {
                    validate158.errors = [{ instancePath: instancePath + "/decodedBytes", schemaPath: "#/properties/decodedBytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                    return false;
                  }
                  if (errors === _errs7) {
                    if (typeof data3 == "number" && isFinite(data3)) {
                      if (data3 > 4194304 || isNaN(data3)) {
                        validate158.errors = [{ instancePath: instancePath + "/decodedBytes", schemaPath: "#/properties/decodedBytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 4194304 }, message: "must be <= 4194304" }];
                        return false;
                      } else {
                        if (data3 < 1 || isNaN(data3)) {
                          validate158.errors = [{ instancePath: instancePath + "/decodedBytes", schemaPath: "#/properties/decodedBytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                          return false;
                        }
                      }
                    }
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.payload !== undefined) {
                    let data4 = data.payload;
                    const _errs9 = errors;
                    if (errors === _errs9) {
                      if (typeof data4 === "string") {
                        if (func3(data4) > 8388608) {
                          validate158.errors = [{ instancePath: instancePath + "/payload", schemaPath: "#/properties/payload/maxLength", keyword: "maxLength", params: { limit: 8388608 }, message: "must NOT have more than 8388608 characters" }];
                          return false;
                        } else {
                          if (func3(data4) < 1) {
                            validate158.errors = [{ instancePath: instancePath + "/payload", schemaPath: "#/properties/payload/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                            return false;
                          }
                        }
                      } else {
                        validate158.errors = [{ instancePath: instancePath + "/payload", schemaPath: "#/properties/payload/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid0 = _errs9 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.authority !== undefined) {
                      const _errs11 = errors;
                      if (data.authority !== "none") {
                        validate158.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/const", keyword: "const", params: { allowedValue: "none" }, message: "must be equal to constant" }];
                        return false;
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.nativePermissionGranted !== undefined) {
                        const _errs12 = errors;
                        if (data.nativePermissionGranted !== false) {
                          validate158.errors = [{ instancePath: instancePath + "/nativePermissionGranted", schemaPath: "#/properties/nativePermissionGranted/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
                          return false;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.semanticAcceptanceEstablished !== undefined) {
                          const _errs13 = errors;
                          if (data.semanticAcceptanceEstablished !== false) {
                            validate158.errors = [{ instancePath: instancePath + "/semanticAcceptanceEstablished", schemaPath: "#/properties/semanticAcceptanceEstablished/const", keyword: "const", params: { allowedValue: false }, message: "must be equal to constant" }];
                            return false;
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
      validate158.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate158.errors = vErrors;
  return errors === 0;
}
export {
  validateAllocationBinding,
  validateCompactContextPacket,
  validateContextPacket,
  validateContract,
  validateCreateRequest,
  validateEffectiveContract,
  validateGraph,
  validateRun,
  validateStoredRecord,
  validateTask,
  validateWork
};
