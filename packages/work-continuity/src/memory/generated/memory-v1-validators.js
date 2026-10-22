/** Generated from memory-v1.schema.json; do not edit. SHA-256: 00b51da4a7b20abdf3ca73a927b79c53bf669b023fc9717c11d01cadef965281; S01: 1f1542021ee00b044bc5d0d7577430aeba58a0d2431aa92fcb7f05c1df7e27e0 */
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

// node_modules/.bun/fast-deep-equal@3.1.3/node_modules/fast-deep-equal/index.js
var require_fast_deep_equal = __commonJS(function(exports, module) {
  module.exports = function equal(a, b) {
    if (a === b)
      return true;
    if (a && b && typeof a == "object" && typeof b == "object") {
      if (a.constructor !== b.constructor)
        return false;
      var length, i, keys;
      if (Array.isArray(a)) {
        length = a.length;
        if (length != b.length)
          return false;
        for (i = length;i-- !== 0; )
          if (!equal(a[i], b[i]))
            return false;
        return true;
      }
      if (a.constructor === RegExp)
        return a.source === b.source && a.flags === b.flags;
      if (a.valueOf !== Object.prototype.valueOf)
        return a.valueOf() === b.valueOf();
      if (a.toString !== Object.prototype.toString)
        return a.toString() === b.toString();
      keys = Object.keys(a);
      length = keys.length;
      if (length !== Object.keys(b).length)
        return false;
      for (i = length;i-- !== 0; )
        if (!Object.prototype.hasOwnProperty.call(b, keys[i]))
          return false;
      for (i = length;i-- !== 0; ) {
        var key = keys[i];
        if (!equal(a[key], b[key]))
          return false;
      }
      return true;
    }
    return a !== a && b !== b;
  };
});

// node_modules/.bun/ajv@8.20.0/node_modules/ajv/dist/runtime/equal.js
var require_equal = __commonJS(function(exports) {
  Object.defineProperty(exports, "__esModule", { value: true });
  var equal = require_fast_deep_equal();
  equal.code = 'require("ajv/dist/runtime/equal").default';
  exports.default = equal;
});

// packages/ontology/scripts/.memory-v1-standalone.generated.js
var validateMemoryRecord = validate63;
var schema8 = { type: "object", properties: { id: { $ref: "#/$defs/Id" }, revision: { $ref: "#/$defs/Revision" }, owner: { $ref: "#/$defs/Scope" }, subjects: { type: "array", items: { $ref: "#/$defs/Scope" }, maxItems: 100 }, kind: { enum: ["preference", "decision", "episode", "procedure", "knowledge"] }, status: { enum: ["candidate", "active", "superseded", "revoked"] }, authority: { enum: ["source-observation", "model-inference", "owner-adopted"] }, sourceRefs: { type: "array", items: { $ref: "#/$defs/SourceRef" }, maxItems: 100, minItems: 1 }, text: { type: "string", minLength: 1, maxLength: 20000 }, allowedReaders: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 100, uniqueItems: true }, validFrom: { $ref: "#/$defs/Timestamp" }, validUntil: { $ref: "#/$defs/Timestamp" } }, required: ["id", "revision", "owner", "subjects", "kind", "status", "authority", "sourceRefs", "text", "allowedReaders"], additionalProperties: false };
var func2 = Object.prototype.hasOwnProperty;
var func3 = require_ucs2length().default;
var func0 = require_equal().default;
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
function validate26(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (!(typeof data == "number" && (!(data % 1) && !isNaN(data)) && isFinite(data))) {
    validate26.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
    return false;
  }
  if (errors === 0) {
    if (typeof data == "number" && isFinite(data)) {
      if (data > 9007199254740991 || isNaN(data)) {
        validate26.errors = [{ instancePath, schemaPath: "#/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
        return false;
      } else {
        if (data < 1 || isNaN(data)) {
          validate26.errors = [{ instancePath, schemaPath: "#/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
          return false;
        }
      }
    }
  }
  validate26.errors = vErrors;
  return errors === 0;
}
function validate28(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
    validate28.errors = vErrors;
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
  validate28.errors = vErrors;
  return errors === 0;
}
var pattern1 = new RegExp("^[a-f0-9]{64}$", "u");
function validate42(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (typeof data === "string") {
      if (!pattern1.test(data)) {
        validate42.errors = [{ instancePath, schemaPath: "#/pattern", keyword: "pattern", params: { pattern: "^[a-f0-9]{64}$" }, message: 'must match pattern "' + "^[a-f0-9]{64}$" + '"' }];
        return false;
      }
    } else {
      validate42.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
      return false;
    }
  }
  validate42.errors = vErrors;
  return errors === 0;
}
function validate39(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.owner === undefined && (missing0 = "owner") || data.uri === undefined && (missing0 = "uri") || data.revision === undefined && (missing0 = "revision") || data.digest === undefined && (missing0 = "digest")) {
        validate39.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "owner" || key0 === "uri" || key0 === "revision" || key0 === "digest")) {
            validate39.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
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
                    validate39.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/maxLength", keyword: "maxLength", params: { limit: 4096 }, message: "must NOT have more than 4096 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate39.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    }
                  }
                } else {
                  validate39.errors = [{ instancePath: instancePath + "/uri", schemaPath: "#/properties/uri/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
                  if (!validate42(data.digest, { instancePath: instancePath + "/digest", parentData: data, parentDataProperty: "digest", rootData })) {
                    vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
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
      validate39.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate39.errors = vErrors;
  return errors === 0;
}
var pattern2 = new RegExp("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\\.[0-9]{3})?Z$", "u");
function validate46(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (typeof data === "string") {
      if (!pattern2.test(data)) {
        validate46.errors = [{ instancePath, schemaPath: "#/pattern", keyword: "pattern", params: { pattern: "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\\.[0-9]{3})?Z$" }, message: 'must match pattern "' + "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\\.[0-9]{3})?Z$" + '"' }];
        return false;
      }
    } else {
      validate46.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
      return false;
    }
  }
  validate46.errors = vErrors;
  return errors === 0;
}
function validate63(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.revision === undefined && (missing0 = "revision") || data.owner === undefined && (missing0 = "owner") || data.subjects === undefined && (missing0 = "subjects") || data.kind === undefined && (missing0 = "kind") || data.status === undefined && (missing0 = "status") || data.authority === undefined && (missing0 = "authority") || data.sourceRefs === undefined && (missing0 = "sourceRefs") || data.text === undefined && (missing0 = "text") || data.allowedReaders === undefined && (missing0 = "allowedReaders")) {
        validate63.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema8.properties, key0)) {
            validate63.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
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
              if (!validate26(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                vErrors = vErrors === null ? validate26.errors : vErrors.concat(validate26.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.owner !== undefined) {
                const _errs4 = errors;
                if (!validate28(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                  vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.subjects !== undefined) {
                  let data3 = data.subjects;
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 100) {
                        validate63.errors = [{ instancePath: instancePath + "/subjects", schemaPath: "#/properties/subjects/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate28(data3[i0], { instancePath: instancePath + "/subjects/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate63.errors = [{ instancePath: instancePath + "/subjects", schemaPath: "#/properties/subjects/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.kind !== undefined) {
                    let data5 = data.kind;
                    const _errs8 = errors;
                    if (!(data5 === "preference" || data5 === "decision" || data5 === "episode" || data5 === "procedure" || data5 === "knowledge")) {
                      validate63.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema8.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid0 = _errs8 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.status !== undefined) {
                      let data6 = data.status;
                      const _errs9 = errors;
                      if (!(data6 === "candidate" || data6 === "active" || data6 === "superseded" || data6 === "revoked")) {
                        validate63.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema8.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                        return false;
                      }
                      var valid0 = _errs9 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.authority !== undefined) {
                        let data7 = data.authority;
                        const _errs10 = errors;
                        if (!(data7 === "source-observation" || data7 === "model-inference" || data7 === "owner-adopted")) {
                          validate63.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/enum", keyword: "enum", params: { allowedValues: schema8.properties.authority.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid0 = _errs10 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.sourceRefs !== undefined) {
                          let data8 = data.sourceRefs;
                          const _errs11 = errors;
                          if (errors === _errs11) {
                            if (Array.isArray(data8)) {
                              if (data8.length > 100) {
                                validate63.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                return false;
                              } else {
                                if (data8.length < 1) {
                                  validate63.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/minItems", keyword: "minItems", params: { limit: 1 }, message: "must NOT have fewer than 1 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data8.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs13 = errors;
                                    if (!validate39(data8[i1], { instancePath: instancePath + "/sourceRefs/" + i1, parentData: data8, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs13 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              }
                            } else {
                              validate63.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs11 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.text !== undefined) {
                            let data10 = data.text;
                            const _errs14 = errors;
                            if (errors === _errs14) {
                              if (typeof data10 === "string") {
                                if (func3(data10) > 20000) {
                                  validate63.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/maxLength", keyword: "maxLength", params: { limit: 20000 }, message: "must NOT have more than 20000 characters" }];
                                  return false;
                                } else {
                                  if (func3(data10) < 1) {
                                    validate63.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                    return false;
                                  }
                                }
                              } else {
                                validate63.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                            }
                            var valid0 = _errs14 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.allowedReaders !== undefined) {
                              let data11 = data.allowedReaders;
                              const _errs16 = errors;
                              if (errors === _errs16) {
                                if (Array.isArray(data11)) {
                                  if (data11.length > 100) {
                                    validate63.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                    return false;
                                  } else {
                                    var valid3 = true;
                                    const len2 = data11.length;
                                    for (let i2 = 0;i2 < len2; i2++) {
                                      const _errs18 = errors;
                                      if (!validate24(data11[i2], { instancePath: instancePath + "/allowedReaders/" + i2, parentData: data11, parentDataProperty: i2, rootData })) {
                                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid3 = _errs18 === errors;
                                      if (!valid3) {
                                        break;
                                      }
                                    }
                                    if (valid3) {
                                      let i3 = data11.length;
                                      let j0;
                                      if (i3 > 1) {
                                        outer0:
                                          for (;i3--; ) {
                                            for (j0 = i3;j0--; ) {
                                              if (func0(data11[i3], data11[j0])) {
                                                validate63.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/uniqueItems", keyword: "uniqueItems", params: { i: i3, j: j0 }, message: "must NOT have duplicate items (items ## " + j0 + " and " + i3 + " are identical)" }];
                                                return false;
                                                break outer0;
                                              }
                                            }
                                          }
                                      }
                                    }
                                  }
                                } else {
                                  validate63.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs16 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.validFrom !== undefined) {
                                const _errs19 = errors;
                                if (!validate46(data.validFrom, { instancePath: instancePath + "/validFrom", parentData: data, parentDataProperty: "validFrom", rootData })) {
                                  vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
                                  errors = vErrors.length;
                                }
                                var valid0 = _errs19 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.validUntil !== undefined) {
                                  const _errs20 = errors;
                                  if (!validate46(data.validUntil, { instancePath: instancePath + "/validUntil", parentData: data, parentDataProperty: "validUntil", rootData })) {
                                    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
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
    } else {
      validate63.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate63.errors = vErrors;
  return errors === 0;
}
var validateMemoryDocument = validate72;
function validate23(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === undefined && (missing0 = "id") || data.revision === undefined && (missing0 = "revision") || data.owner === undefined && (missing0 = "owner") || data.subjects === undefined && (missing0 = "subjects") || data.kind === undefined && (missing0 = "kind") || data.status === undefined && (missing0 = "status") || data.authority === undefined && (missing0 = "authority") || data.sourceRefs === undefined && (missing0 = "sourceRefs") || data.text === undefined && (missing0 = "text") || data.allowedReaders === undefined && (missing0 = "allowedReaders")) {
        validate23.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema8.properties, key0)) {
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
            if (data.revision !== undefined) {
              const _errs3 = errors;
              if (!validate26(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                vErrors = vErrors === null ? validate26.errors : vErrors.concat(validate26.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.owner !== undefined) {
                const _errs4 = errors;
                if (!validate28(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                  vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.subjects !== undefined) {
                  let data3 = data.subjects;
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (Array.isArray(data3)) {
                      if (data3.length > 100) {
                        validate23.errors = [{ instancePath: instancePath + "/subjects", schemaPath: "#/properties/subjects/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                        return false;
                      } else {
                        var valid1 = true;
                        const len0 = data3.length;
                        for (let i0 = 0;i0 < len0; i0++) {
                          const _errs7 = errors;
                          if (!validate28(data3[i0], { instancePath: instancePath + "/subjects/" + i0, parentData: data3, parentDataProperty: i0, rootData })) {
                            vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
                            errors = vErrors.length;
                          }
                          var valid1 = _errs7 === errors;
                          if (!valid1) {
                            break;
                          }
                        }
                      }
                    } else {
                      validate23.errors = [{ instancePath: instancePath + "/subjects", schemaPath: "#/properties/subjects/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.kind !== undefined) {
                    let data5 = data.kind;
                    const _errs8 = errors;
                    if (!(data5 === "preference" || data5 === "decision" || data5 === "episode" || data5 === "procedure" || data5 === "knowledge")) {
                      validate23.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/enum", keyword: "enum", params: { allowedValues: schema8.properties.kind.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid0 = _errs8 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.status !== undefined) {
                      let data6 = data.status;
                      const _errs9 = errors;
                      if (!(data6 === "candidate" || data6 === "active" || data6 === "superseded" || data6 === "revoked")) {
                        validate23.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema8.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                        return false;
                      }
                      var valid0 = _errs9 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.authority !== undefined) {
                        let data7 = data.authority;
                        const _errs10 = errors;
                        if (!(data7 === "source-observation" || data7 === "model-inference" || data7 === "owner-adopted")) {
                          validate23.errors = [{ instancePath: instancePath + "/authority", schemaPath: "#/properties/authority/enum", keyword: "enum", params: { allowedValues: schema8.properties.authority.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid0 = _errs10 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.sourceRefs !== undefined) {
                          let data8 = data.sourceRefs;
                          const _errs11 = errors;
                          if (errors === _errs11) {
                            if (Array.isArray(data8)) {
                              if (data8.length > 100) {
                                validate23.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                return false;
                              } else {
                                if (data8.length < 1) {
                                  validate23.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/minItems", keyword: "minItems", params: { limit: 1 }, message: "must NOT have fewer than 1 items" }];
                                  return false;
                                } else {
                                  var valid2 = true;
                                  const len1 = data8.length;
                                  for (let i1 = 0;i1 < len1; i1++) {
                                    const _errs13 = errors;
                                    if (!validate39(data8[i1], { instancePath: instancePath + "/sourceRefs/" + i1, parentData: data8, parentDataProperty: i1, rootData })) {
                                      vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                                      errors = vErrors.length;
                                    }
                                    var valid2 = _errs13 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                }
                              }
                            } else {
                              validate23.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs11 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.text !== undefined) {
                            let data10 = data.text;
                            const _errs14 = errors;
                            if (errors === _errs14) {
                              if (typeof data10 === "string") {
                                if (func3(data10) > 20000) {
                                  validate23.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/maxLength", keyword: "maxLength", params: { limit: 20000 }, message: "must NOT have more than 20000 characters" }];
                                  return false;
                                } else {
                                  if (func3(data10) < 1) {
                                    validate23.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                    return false;
                                  }
                                }
                              } else {
                                validate23.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                            }
                            var valid0 = _errs14 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.allowedReaders !== undefined) {
                              let data11 = data.allowedReaders;
                              const _errs16 = errors;
                              if (errors === _errs16) {
                                if (Array.isArray(data11)) {
                                  if (data11.length > 100) {
                                    validate23.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                    return false;
                                  } else {
                                    var valid3 = true;
                                    const len2 = data11.length;
                                    for (let i2 = 0;i2 < len2; i2++) {
                                      const _errs18 = errors;
                                      if (!validate24(data11[i2], { instancePath: instancePath + "/allowedReaders/" + i2, parentData: data11, parentDataProperty: i2, rootData })) {
                                        vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                        errors = vErrors.length;
                                      }
                                      var valid3 = _errs18 === errors;
                                      if (!valid3) {
                                        break;
                                      }
                                    }
                                    if (valid3) {
                                      let i3 = data11.length;
                                      let j0;
                                      if (i3 > 1) {
                                        outer0:
                                          for (;i3--; ) {
                                            for (j0 = i3;j0--; ) {
                                              if (func0(data11[i3], data11[j0])) {
                                                validate23.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/uniqueItems", keyword: "uniqueItems", params: { i: i3, j: j0 }, message: "must NOT have duplicate items (items ## " + j0 + " and " + i3 + " are identical)" }];
                                                return false;
                                                break outer0;
                                              }
                                            }
                                          }
                                      }
                                    }
                                  }
                                } else {
                                  validate23.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs16 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.validFrom !== undefined) {
                                const _errs19 = errors;
                                if (!validate46(data.validFrom, { instancePath: instancePath + "/validFrom", parentData: data, parentDataProperty: "validFrom", rootData })) {
                                  vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
                                  errors = vErrors.length;
                                }
                                var valid0 = _errs19 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.validUntil !== undefined) {
                                  const _errs20 = errors;
                                  if (!validate46(data.validUntil, { instancePath: instancePath + "/validUntil", parentData: data, parentDataProperty: "validUntil", rootData })) {
                                    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
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
    } else {
      validate23.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate23.errors = vErrors;
  return errors === 0;
}
function validate50(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.key === undefined && (missing0 = "key") || data.value === undefined && (missing0 = "value")) {
        validate50.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "key" || key0 === "value")) {
            validate50.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.key !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.key, { instancePath: instancePath + "/key", parentData: data, parentDataProperty: "key", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.value !== undefined) {
              let data1 = data.value;
              const _errs3 = errors;
              if (errors === _errs3) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 4000) {
                    validate50.errors = [{ instancePath: instancePath + "/value", schemaPath: "#/properties/value/maxLength", keyword: "maxLength", params: { limit: 4000 }, message: "must NOT have more than 4000 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate50.errors = [{ instancePath: instancePath + "/value", schemaPath: "#/properties/value/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    }
                  }
                } else {
                  validate50.errors = [{ instancePath: instancePath + "/value", schemaPath: "#/properties/value/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate50.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate50.errors = vErrors;
  return errors === 0;
}
var wrapper0 = { validate: validate59 };
function validate59(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
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
    validate59.errors = vErrors;
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
  validate59.errors = vErrors;
  return errors === 0;
}
function validate53(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.protocol === undefined && (missing0 = "protocol") || data.identityKey === undefined && (missing0 = "identityKey") || data.original === undefined && (missing0 = "original") || data.metadata === undefined && (missing0 = "metadata")) {
        validate53.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "protocol" || key0 === "identityKey" || key0 === "generationId" || key0 === "claimKey" || key0 === "original" || key0 === "metadata")) {
            validate53.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.protocol !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.protocol, { instancePath: instancePath + "/protocol", parentData: data, parentDataProperty: "protocol", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.identityKey !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.identityKey, { instancePath: instancePath + "/identityKey", parentData: data, parentDataProperty: "identityKey", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.generationId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.generationId, { instancePath: instancePath + "/generationId", parentData: data, parentDataProperty: "generationId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.claimKey !== undefined) {
                  const _errs5 = errors;
                  if (!validate24(data.claimKey, { instancePath: instancePath + "/claimKey", parentData: data, parentDataProperty: "claimKey", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.original !== undefined) {
                    const _errs6 = errors;
                    if (!validate39(data.original, { instancePath: instancePath + "/original", parentData: data, parentDataProperty: "original", rootData })) {
                      vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.metadata !== undefined) {
                      const _errs7 = errors;
                      if (!validate59(data.metadata, { instancePath: instancePath + "/metadata", parentData: data, parentDataProperty: "metadata", rootData })) {
                        vErrors = vErrors === null ? validate59.errors : vErrors.concat(validate59.errors);
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
      validate53.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate53.errors = vErrors;
  return errors === 0;
}
function validate72(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.record === undefined && (missing0 = "record")) {
        validate72.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schema" || key0 === "record" || key0 === "claim" || key0 === "legacy")) {
            validate72.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.memory-document.v1") {
              validate72.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.memory-document.v1" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.record !== undefined) {
              const _errs3 = errors;
              if (!validate23(data.record, { instancePath: instancePath + "/record", parentData: data, parentDataProperty: "record", rootData })) {
                vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.claim !== undefined) {
                const _errs4 = errors;
                if (!validate50(data.claim, { instancePath: instancePath + "/claim", parentData: data, parentDataProperty: "claim", rootData })) {
                  vErrors = vErrors === null ? validate50.errors : vErrors.concat(validate50.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.legacy !== undefined) {
                  const _errs5 = errors;
                  if (!validate53(data.legacy, { instancePath: instancePath + "/legacy", parentData: data, parentDataProperty: "legacy", rootData })) {
                    vErrors = vErrors === null ? validate53.errors : vErrors.concat(validate53.errors);
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
      validate72.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate72.errors = vErrors;
  return errors === 0;
}
var validateMemoryQuery = validate76;
var schema20 = { type: "object", properties: { schema: { const: "common.memory-query.v1" }, purpose: { type: "string", minLength: 1, maxLength: 1000 }, query: { type: "string", minLength: 1, maxLength: 500 }, mode: { enum: ["current", "history"] }, scopes: { type: "array", items: { $ref: "#/$defs/Scope" }, maxItems: 100 }, work: { $ref: "#/$defs/WorkRef" }, task: { $ref: "#/$defs/TaskRef" }, effectiveDigest: { $ref: "#/$defs/Digest" }, transport: { enum: ["full", "compact", "dispatch", "resume"] }, asOf: { $ref: "#/$defs/Timestamp" }, limit: { type: "integer", minimum: 1, maximum: 20 }, maxChars: { type: "integer", minimum: 512, maximum: 64000 } }, required: ["schema", "purpose", "query", "mode"], additionalProperties: false };
function validate78(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId")) {
        validate78.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId")) {
            validate78.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
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
      validate78.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate78.errors = vErrors;
  return errors === 0;
}
function validate82(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.projectId === undefined && (missing0 = "projectId") || data.workId === undefined && (missing0 = "workId") || data.taskId === undefined && (missing0 = "taskId")) {
        validate82.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "projectId" || key0 === "workId" || key0 === "taskId")) {
            validate82.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
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
      validate82.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate82.errors = vErrors;
  return errors === 0;
}
function validate76(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.purpose === undefined && (missing0 = "purpose") || data.query === undefined && (missing0 = "query") || data.mode === undefined && (missing0 = "mode")) {
        validate76.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema20.properties, key0)) {
            validate76.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.memory-query.v1") {
              validate76.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.memory-query.v1" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.purpose !== undefined) {
              let data1 = data.purpose;
              const _errs3 = errors;
              if (errors === _errs3) {
                if (typeof data1 === "string") {
                  if (func3(data1) > 1000) {
                    validate76.errors = [{ instancePath: instancePath + "/purpose", schemaPath: "#/properties/purpose/maxLength", keyword: "maxLength", params: { limit: 1000 }, message: "must NOT have more than 1000 characters" }];
                    return false;
                  } else {
                    if (func3(data1) < 1) {
                      validate76.errors = [{ instancePath: instancePath + "/purpose", schemaPath: "#/properties/purpose/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    }
                  }
                } else {
                  validate76.errors = [{ instancePath: instancePath + "/purpose", schemaPath: "#/properties/purpose/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.query !== undefined) {
                let data2 = data.query;
                const _errs5 = errors;
                if (errors === _errs5) {
                  if (typeof data2 === "string") {
                    if (func3(data2) > 500) {
                      validate76.errors = [{ instancePath: instancePath + "/query", schemaPath: "#/properties/query/maxLength", keyword: "maxLength", params: { limit: 500 }, message: "must NOT have more than 500 characters" }];
                      return false;
                    } else {
                      if (func3(data2) < 1) {
                        validate76.errors = [{ instancePath: instancePath + "/query", schemaPath: "#/properties/query/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      }
                    }
                  } else {
                    validate76.errors = [{ instancePath: instancePath + "/query", schemaPath: "#/properties/query/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.mode !== undefined) {
                  let data3 = data.mode;
                  const _errs7 = errors;
                  if (!(data3 === "current" || data3 === "history")) {
                    validate76.errors = [{ instancePath: instancePath + "/mode", schemaPath: "#/properties/mode/enum", keyword: "enum", params: { allowedValues: schema20.properties.mode.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.scopes !== undefined) {
                    let data4 = data.scopes;
                    const _errs8 = errors;
                    if (errors === _errs8) {
                      if (Array.isArray(data4)) {
                        if (data4.length > 100) {
                          validate76.errors = [{ instancePath: instancePath + "/scopes", schemaPath: "#/properties/scopes/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                          return false;
                        } else {
                          var valid1 = true;
                          const len0 = data4.length;
                          for (let i0 = 0;i0 < len0; i0++) {
                            const _errs10 = errors;
                            if (!validate28(data4[i0], { instancePath: instancePath + "/scopes/" + i0, parentData: data4, parentDataProperty: i0, rootData })) {
                              vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
                              errors = vErrors.length;
                            }
                            var valid1 = _errs10 === errors;
                            if (!valid1) {
                              break;
                            }
                          }
                        }
                      } else {
                        validate76.errors = [{ instancePath: instancePath + "/scopes", schemaPath: "#/properties/scopes/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                        return false;
                      }
                    }
                    var valid0 = _errs8 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.work !== undefined) {
                      const _errs11 = errors;
                      if (!validate78(data.work, { instancePath: instancePath + "/work", parentData: data, parentDataProperty: "work", rootData })) {
                        vErrors = vErrors === null ? validate78.errors : vErrors.concat(validate78.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.task !== undefined) {
                        const _errs12 = errors;
                        if (!validate82(data.task, { instancePath: instancePath + "/task", parentData: data, parentDataProperty: "task", rootData })) {
                          vErrors = vErrors === null ? validate82.errors : vErrors.concat(validate82.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.effectiveDigest !== undefined) {
                          const _errs13 = errors;
                          if (!validate42(data.effectiveDigest, { instancePath: instancePath + "/effectiveDigest", parentData: data, parentDataProperty: "effectiveDigest", rootData })) {
                            vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
                            errors = vErrors.length;
                          }
                          var valid0 = _errs13 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.transport !== undefined) {
                            let data9 = data.transport;
                            const _errs14 = errors;
                            if (!(data9 === "full" || data9 === "compact" || data9 === "dispatch" || data9 === "resume")) {
                              validate76.errors = [{ instancePath: instancePath + "/transport", schemaPath: "#/properties/transport/enum", keyword: "enum", params: { allowedValues: schema20.properties.transport.enum }, message: "must be equal to one of the allowed values" }];
                              return false;
                            }
                            var valid0 = _errs14 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.asOf !== undefined) {
                              const _errs15 = errors;
                              if (!validate46(data.asOf, { instancePath: instancePath + "/asOf", parentData: data, parentDataProperty: "asOf", rootData })) {
                                vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
                                errors = vErrors.length;
                              }
                              var valid0 = _errs15 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.limit !== undefined) {
                                let data11 = data.limit;
                                const _errs16 = errors;
                                if (!(typeof data11 == "number" && (!(data11 % 1) && !isNaN(data11)) && isFinite(data11))) {
                                  validate76.errors = [{ instancePath: instancePath + "/limit", schemaPath: "#/properties/limit/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                  return false;
                                }
                                if (errors === _errs16) {
                                  if (typeof data11 == "number" && isFinite(data11)) {
                                    if (data11 > 20 || isNaN(data11)) {
                                      validate76.errors = [{ instancePath: instancePath + "/limit", schemaPath: "#/properties/limit/maximum", keyword: "maximum", params: { comparison: "<=", limit: 20 }, message: "must be <= 20" }];
                                      return false;
                                    } else {
                                      if (data11 < 1 || isNaN(data11)) {
                                        validate76.errors = [{ instancePath: instancePath + "/limit", schemaPath: "#/properties/limit/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                        return false;
                                      }
                                    }
                                  }
                                }
                                var valid0 = _errs16 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.maxChars !== undefined) {
                                  let data12 = data.maxChars;
                                  const _errs18 = errors;
                                  if (!(typeof data12 == "number" && (!(data12 % 1) && !isNaN(data12)) && isFinite(data12))) {
                                    validate76.errors = [{ instancePath: instancePath + "/maxChars", schemaPath: "#/properties/maxChars/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                    return false;
                                  }
                                  if (errors === _errs18) {
                                    if (typeof data12 == "number" && isFinite(data12)) {
                                      if (data12 > 64000 || isNaN(data12)) {
                                        validate76.errors = [{ instancePath: instancePath + "/maxChars", schemaPath: "#/properties/maxChars/maximum", keyword: "maximum", params: { comparison: "<=", limit: 64000 }, message: "must be <= 64000" }];
                                        return false;
                                      } else {
                                        if (data12 < 512 || isNaN(data12)) {
                                          validate76.errors = [{ instancePath: instancePath + "/maxChars", schemaPath: "#/properties/maxChars/minimum", keyword: "minimum", params: { comparison: ">=", limit: 512 }, message: "must be >= 512" }];
                                          return false;
                                        }
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
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate76.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate76.errors = vErrors;
  return errors === 0;
}
var validateGoverningDocument = validate89;
var schema23 = { type: "object", properties: { schema: { const: "common.memory-governing.v1" }, id: { $ref: "#/$defs/Id" }, revision: { $ref: "#/$defs/Revision" }, owner: { $ref: "#/$defs/Scope" }, recordClass: { enum: ["decision", "spec"] }, status: { enum: ["active", "superseded", "revoked"] }, sourceRefs: { type: "array", items: { $ref: "#/$defs/SourceRef" }, maxItems: 100, minItems: 1 }, allowedReaders: { type: "array", items: { $ref: "#/$defs/Id" }, maxItems: 100, uniqueItems: true }, claim: { $ref: "#/$defs/Claim" }, text: { type: "string", minLength: 1, maxLength: 20000 }, validFrom: { $ref: "#/$defs/Timestamp" }, validUntil: { $ref: "#/$defs/Timestamp" } }, required: ["schema", "id", "revision", "owner", "recordClass", "status", "sourceRefs", "allowedReaders", "claim", "text"], additionalProperties: false };
function validate89(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schema === undefined && (missing0 = "schema") || data.id === undefined && (missing0 = "id") || data.revision === undefined && (missing0 = "revision") || data.owner === undefined && (missing0 = "owner") || data.recordClass === undefined && (missing0 = "recordClass") || data.status === undefined && (missing0 = "status") || data.sourceRefs === undefined && (missing0 = "sourceRefs") || data.allowedReaders === undefined && (missing0 = "allowedReaders") || data.claim === undefined && (missing0 = "claim") || data.text === undefined && (missing0 = "text")) {
        validate89.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func2.call(schema23.properties, key0)) {
            validate89.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schema !== undefined) {
            const _errs2 = errors;
            if (data.schema !== "common.memory-governing.v1") {
              validate89.errors = [{ instancePath: instancePath + "/schema", schemaPath: "#/properties/schema/const", keyword: "const", params: { allowedValue: "common.memory-governing.v1" }, message: "must be equal to constant" }];
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
                if (!validate26(data.revision, { instancePath: instancePath + "/revision", parentData: data, parentDataProperty: "revision", rootData })) {
                  vErrors = vErrors === null ? validate26.errors : vErrors.concat(validate26.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.owner !== undefined) {
                  const _errs5 = errors;
                  if (!validate28(data.owner, { instancePath: instancePath + "/owner", parentData: data, parentDataProperty: "owner", rootData })) {
                    vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.recordClass !== undefined) {
                    let data4 = data.recordClass;
                    const _errs6 = errors;
                    if (!(data4 === "decision" || data4 === "spec")) {
                      validate89.errors = [{ instancePath: instancePath + "/recordClass", schemaPath: "#/properties/recordClass/enum", keyword: "enum", params: { allowedValues: schema23.properties.recordClass.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.status !== undefined) {
                      let data5 = data.status;
                      const _errs7 = errors;
                      if (!(data5 === "active" || data5 === "superseded" || data5 === "revoked")) {
                        validate89.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema23.properties.status.enum }, message: "must be equal to one of the allowed values" }];
                        return false;
                      }
                      var valid0 = _errs7 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.sourceRefs !== undefined) {
                        let data6 = data.sourceRefs;
                        const _errs8 = errors;
                        if (errors === _errs8) {
                          if (Array.isArray(data6)) {
                            if (data6.length > 100) {
                              validate89.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                              return false;
                            } else {
                              if (data6.length < 1) {
                                validate89.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/minItems", keyword: "minItems", params: { limit: 1 }, message: "must NOT have fewer than 1 items" }];
                                return false;
                              } else {
                                var valid1 = true;
                                const len0 = data6.length;
                                for (let i0 = 0;i0 < len0; i0++) {
                                  const _errs10 = errors;
                                  if (!validate39(data6[i0], { instancePath: instancePath + "/sourceRefs/" + i0, parentData: data6, parentDataProperty: i0, rootData })) {
                                    vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid1 = _errs10 === errors;
                                  if (!valid1) {
                                    break;
                                  }
                                }
                              }
                            }
                          } else {
                            validate89.errors = [{ instancePath: instancePath + "/sourceRefs", schemaPath: "#/properties/sourceRefs/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                            return false;
                          }
                        }
                        var valid0 = _errs8 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.allowedReaders !== undefined) {
                          let data8 = data.allowedReaders;
                          const _errs11 = errors;
                          if (errors === _errs11) {
                            if (Array.isArray(data8)) {
                              if (data8.length > 100) {
                                validate89.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                                return false;
                              } else {
                                var valid2 = true;
                                const len1 = data8.length;
                                for (let i1 = 0;i1 < len1; i1++) {
                                  const _errs13 = errors;
                                  if (!validate24(data8[i1], { instancePath: instancePath + "/allowedReaders/" + i1, parentData: data8, parentDataProperty: i1, rootData })) {
                                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid2 = _errs13 === errors;
                                  if (!valid2) {
                                    break;
                                  }
                                }
                                if (valid2) {
                                  let i2 = data8.length;
                                  let j0;
                                  if (i2 > 1) {
                                    outer0:
                                      for (;i2--; ) {
                                        for (j0 = i2;j0--; ) {
                                          if (func0(data8[i2], data8[j0])) {
                                            validate89.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/uniqueItems", keyword: "uniqueItems", params: { i: i2, j: j0 }, message: "must NOT have duplicate items (items ## " + j0 + " and " + i2 + " are identical)" }];
                                            return false;
                                            break outer0;
                                          }
                                        }
                                      }
                                  }
                                }
                              }
                            } else {
                              validate89.errors = [{ instancePath: instancePath + "/allowedReaders", schemaPath: "#/properties/allowedReaders/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs11 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.claim !== undefined) {
                            const _errs14 = errors;
                            if (!validate50(data.claim, { instancePath: instancePath + "/claim", parentData: data, parentDataProperty: "claim", rootData })) {
                              vErrors = vErrors === null ? validate50.errors : vErrors.concat(validate50.errors);
                              errors = vErrors.length;
                            }
                            var valid0 = _errs14 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.text !== undefined) {
                              let data11 = data.text;
                              const _errs15 = errors;
                              if (errors === _errs15) {
                                if (typeof data11 === "string") {
                                  if (func3(data11) > 20000) {
                                    validate89.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/maxLength", keyword: "maxLength", params: { limit: 20000 }, message: "must NOT have more than 20000 characters" }];
                                    return false;
                                  } else {
                                    if (func3(data11) < 1) {
                                      validate89.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                      return false;
                                    }
                                  }
                                } else {
                                  validate89.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs15 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.validFrom !== undefined) {
                                const _errs17 = errors;
                                if (!validate46(data.validFrom, { instancePath: instancePath + "/validFrom", parentData: data, parentDataProperty: "validFrom", rootData })) {
                                  vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
                                  errors = vErrors.length;
                                }
                                var valid0 = _errs17 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.validUntil !== undefined) {
                                  const _errs18 = errors;
                                  if (!validate46(data.validUntil, { instancePath: instancePath + "/validUntil", parentData: data, parentDataProperty: "validUntil", rootData })) {
                                    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
                                    errors = vErrors.length;
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
              }
            }
          }
        }
      }
    } else {
      validate89.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate89.errors = vErrors;
  return errors === 0;
}
var validateLegacyBinding = validate98;
function validate98(data, { instancePath = "", parentData, parentDataProperty, rootData = data } = {}) {
  let vErrors = null;
  let errors = 0;
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.protocol === undefined && (missing0 = "protocol") || data.identityKey === undefined && (missing0 = "identityKey") || data.original === undefined && (missing0 = "original") || data.metadata === undefined && (missing0 = "metadata")) {
        validate98.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "protocol" || key0 === "identityKey" || key0 === "generationId" || key0 === "claimKey" || key0 === "original" || key0 === "metadata")) {
            validate98.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.protocol !== undefined) {
            const _errs2 = errors;
            if (!validate24(data.protocol, { instancePath: instancePath + "/protocol", parentData: data, parentDataProperty: "protocol", rootData })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.identityKey !== undefined) {
              const _errs3 = errors;
              if (!validate24(data.identityKey, { instancePath: instancePath + "/identityKey", parentData: data, parentDataProperty: "identityKey", rootData })) {
                vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.generationId !== undefined) {
                const _errs4 = errors;
                if (!validate24(data.generationId, { instancePath: instancePath + "/generationId", parentData: data, parentDataProperty: "generationId", rootData })) {
                  vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.claimKey !== undefined) {
                  const _errs5 = errors;
                  if (!validate24(data.claimKey, { instancePath: instancePath + "/claimKey", parentData: data, parentDataProperty: "claimKey", rootData })) {
                    vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
                    errors = vErrors.length;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.original !== undefined) {
                    const _errs6 = errors;
                    if (!validate39(data.original, { instancePath: instancePath + "/original", parentData: data, parentDataProperty: "original", rootData })) {
                      vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs6 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.metadata !== undefined) {
                      const _errs7 = errors;
                      if (!validate59(data.metadata, { instancePath: instancePath + "/metadata", parentData: data, parentDataProperty: "metadata", rootData })) {
                        vErrors = vErrors === null ? validate59.errors : vErrors.concat(validate59.errors);
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
      validate98.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate98.errors = vErrors;
  return errors === 0;
}
export {
  validateGoverningDocument,
  validateLegacyBinding,
  validateMemoryDocument,
  validateMemoryQuery,
  validateMemoryRecord
};
