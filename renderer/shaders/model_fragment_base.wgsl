struct ProjectionData {
    modelMatrix: mat4x4<f32>,
    projectionMatrix: mat4x4<f32>,
    modelViewMatrix: mat4x4<f32>,
    normalMatrix: mat3x3<f32>,
    viewMatrix: mat4x4<f32>,
    isOrthographic: i32,
}

struct MaterialUniforms {
    diffuse: vec3<f32>,
    emissive: vec3<f32>,
    roughness: f32,
    metalness: f32,
}

@group(0) @binding(0)
var<uniform> projection: ProjectionData;

@group(1) @binding(0)
var<uniform> materialUniforms: MaterialUniforms;

@group(1) @binding(1)
var envMapTexture: texture_2d<f32>;

@group(1) @binding(2)
var envMapSampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @builtin(front_facing) frontFacing: bool,
    @location(0) viewPosition: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
}

struct ReflectedLight {
    directDiffuse: vec3<f32>,
    directSpecular: vec3<f32>,
    indirectDiffuse: vec3<f32>,
    indirectSpecular: vec3<f32>,
}

struct GeometricContext {
    position: vec3<f32>,
    normal: vec3<f32>,
    viewDir: vec3<f32>,
}

struct PhysicalMaterial {
    diffuseColor: vec3<f32>,
    roughness: f32,
    specularColor: vec3<f32>,
    specularF90: f32,
}

fn getAmbientLightIrradiance(in: vec3<f32>) -> vec3<f32> {
    return in;
}

fn inverseTransformDirection(dir: vec3<f32>, matrix: mat4x4<f32>) -> vec3<f32> {
    return normalize((vec4<f32>(dir, 0.0) * matrix).xyz);
}

const PI: f32 = 3.141592653589793;
const RECIPROCAL_PI: f32 = 0.3183098861837907;

const cubeUV_maxMipLevel: f32 = 8.0;
const cubeUV_minMipLevel: f32 = 4.0;
const cubeUV_maxTileSize: f32 = 256.0;
const cubeUV_minTileSize: f32 = 16.0;

fn getFace(direction: vec3<f32>) -> f32 {
    let absDirection = abs(direction);
    var face = -1.0;
    if (absDirection.x > absDirection.z) {
        if (absDirection.x > absDirection.y) {
            face = select(3.0, 0.0, direction.x > 0.0);
        } else {
            face = select(4.0, 1.0, direction.y > 0.0);
        }
    } else {
        if (absDirection.z > absDirection.y) {
            face = select(5.0, 2.0, direction.z > 0.0);
        } else {
            face = select(4.0, 1.0, direction.y > 0.0);
        }
    }
    return face;
}

fn getUV(direction: vec3<f32>, face: f32) -> vec2<f32> {
    var uv: vec2<f32>;
    if (face == 0.0) {
        uv = vec2<f32>(direction.z, direction.y) / abs(direction.x);
    } else if (face == 1.0) {
        uv = vec2<f32>(-direction.x, -direction.z) / abs(direction.y);
    } else if (face == 2.0) {
        uv = vec2<f32>(-direction.x, direction.y) / abs(direction.z);
    } else if (face == 3.0) {
        uv = vec2<f32>(-direction.z, direction.y) / abs(direction.x);
    } else if (face == 4.0) {
        uv = vec2<f32>(-direction.x, direction.z) / abs(direction.y);
    } else {
        uv = vec2(direction.x, direction.y) / abs(direction.z);
    }
    return 0.5 * (uv + 1.0);
}

fn envMapTexelToLinear(value: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(value.rgb * exp2(value.a * 255.0 - 128.0), 1.0);
}

fn bilinearCubeUV(envMapTexture: texture_2d<f32>, envMapSampler: sampler, direction: vec3<f32>, mipInt_: f32) -> vec3<f32> {
    var face = getFace(direction);
    let filterInt = max(cubeUV_minMipLevel - mipInt_, 0.0);
    let mipInt = max(mipInt_, cubeUV_minMipLevel);
    let faceSize = exp2(mipInt);
    let texelSize = 1.0 / (3.0 * cubeUV_maxTileSize);
    var uv = getUV(direction, face) * (faceSize - 1.0);
    let f = fract(uv);
    uv += 0.5 - f;
    if (face > 2.0) {
        uv.y += faceSize;
        face -= 3.0;
    }
    uv.x += face * faceSize;
    if (mipInt < cubeUV_maxMipLevel) {
        uv.y += 2.0 * cubeUV_maxTileSize;
    }
    uv.y += filterInt * 2.0 * cubeUV_minTileSize;
    uv.x += 3.0 * max(0.0, cubeUV_maxTileSize - 2.0 * faceSize);
    uv *= texelSize;
    let tl = envMapTexelToLinear(textureSample(envMapTexture, envMapSampler, uv)).rgb;
    uv.x += texelSize;
    let tr = envMapTexelToLinear(textureSample(envMapTexture, envMapSampler, uv)).rgb;
    uv.y += texelSize;
    let br = envMapTexelToLinear(textureSample(envMapTexture, envMapSampler, uv)).rgb;
    uv.x -= texelSize;
    let bl = envMapTexelToLinear(textureSample(envMapTexture, envMapSampler, uv)).rgb;
    let tm = mix(tl, tr, f.x);
    let bm = mix(bl, br, f.x);

    return mix(tm, bm, f.y);
}

const r0: f32 = 1.0;
const v0: f32 = 0.339;
const m0: f32 = -2.0;
const r1: f32 = 0.8;
const v1: f32 = 0.276;
const m1: f32 = -1.0;
const r4: f32 = 0.4;
const v4: f32 = 0.046;
const m4: f32 = 2.0;
const r5: f32 = 0.305;
const v5: f32 = 0.016;
const m5: f32 = 3.0;
const r6: f32 = 0.21;
const v6: f32 = 0.0038;
const m6: f32 = 4.0;

fn roughnessToMip(roughness: f32) -> f32 {
    if (roughness >= r1) {
        return (r0 - roughness) * (m1 - m0) / (r0 - r1) + m0;
    } else if (roughness >= r4) {
        return (r1 - roughness) * (m4 - m1) / (r1 - r4) + m1;
    } else if (roughness >= r5) {
        return (r4 - roughness) * (m5 - m4) / (r4 - r5) + m4;
    } else if (roughness >= r6) {
        return (r5 - roughness) * (m6 - m5) / (r5 - r6) + m5;
    } else {
        return -2.0 * log2(1.16 * roughness);
    }
}

fn textureCubeUV(envMapTexture: texture_2d<f32>, envMapSampler: sampler, sampleDir: vec3<f32>, roughness: f32) -> vec4<f32> {
    var direction = sampleDir;
    let mip = clamp(roughnessToMip(roughness), m0, cubeUV_maxMipLevel);
    let mipF = fract(mip);
    let mipInt = floor(mip);
    let color0 = bilinearCubeUV(envMapTexture, envMapSampler, direction, mipInt);
    if (mipF == 0.0) {
        return vec4<f32>(color0, 1.0);
    } else {
        let color1 = bilinearCubeUV(envMapTexture, envMapSampler, direction, mipInt + 1.0);
        return vec4<f32>(mix(color0, color1, mipF), 1.0);
    }
}

const envMapIntensity: f32 = 1.0;
const ambientLightColor: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);

fn getIBLIrradiance(normal: vec3<f32>) -> vec3<f32> {
    let worldNormal = inverseTransformDirection(normal, projection.viewMatrix);
    let envMapColor = textureCubeUV(envMapTexture, envMapSampler, worldNormal, 1.0 );
    return PI * envMapColor.rgb * envMapIntensity;
}

fn getIBLRadiance(viewDir: vec3<f32>, normal: vec3<f32>, roughness: f32) -> vec3<f32> {
    var reflectVec: vec3<f32>;
    reflectVec = reflect(-viewDir, normal);
    reflectVec = normalize(mix(reflectVec, normal, roughness * roughness));
    reflectVec = inverseTransformDirection(reflectVec, projection.viewMatrix);
    let envMapColor = textureCubeUV(envMapTexture, envMapSampler, reflectVec, roughness);
    return envMapColor.rgb * envMapIntensity;
}

fn BRDF_Lambert(diffuseColor: vec3<f32>) -> vec3<f32> {
    return RECIPROCAL_PI * diffuseColor;
}

fn DFGApprox(normal: vec3<f32>, viewDir: vec3<f32>, roughness: f32) -> vec2<f32> {
    let dotNV = saturate(dot(normal, viewDir));
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * dotNV)) * r.x + r.y;
    let fab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return fab;
}

fn computeMultiscattering(normal: vec3<f32>, viewDir: vec3<f32>, specularColor: vec3<f32>, specularF90: f32, roughness: f32, singleScatter: ptr<function, vec3<f32>>, multiScatter: ptr<function, vec3<f32>>) {
    let fab = DFGApprox(normal, viewDir, roughness);
    let FssEss = specularColor * fab.x + specularF90 * fab.y;
    let Ess = fab.x + fab.y;
    let Ems = 1.0 - Ess;
    let Favg = specularColor + (1.0 - specularColor) * 0.047619;
    let Fms = FssEss * Favg / (1.0 - Ems * Favg);
    *singleScatter += FssEss;
    *multiScatter += Fms * Ems;
}

fn RE_IndirectDiffuse(irradiance: vec3<f32>, geometry: GeometricContext, material: PhysicalMaterial, reflectedLight: ptr<function, ReflectedLight>) {
    (*reflectedLight).indirectDiffuse += irradiance * BRDF_Lambert(material.diffuseColor);
}

fn RE_IndirectSpecular(radiance: vec3<f32>, irradiance: vec3<f32>, clearcoatRadiance: vec3<f32>, geometry: GeometricContext, material: PhysicalMaterial, reflectedLight: ptr<function, ReflectedLight>) {
    var singleScattering = vec3<f32>(0.0);
    var multiScattering = vec3<f32>(0.0);
    let cosineWeightedIrradiance = irradiance * RECIPROCAL_PI;
    computeMultiscattering(geometry.normal, geometry.viewDir, material.specularColor, material.specularF90, material.roughness, &singleScattering, &multiScattering);
    let diffuse = material.diffuseColor * (1.0 - (singleScattering + multiScattering));
    (*reflectedLight).indirectSpecular += radiance * singleScattering;
    (*reflectedLight).indirectSpecular += multiScattering * cosineWeightedIrradiance;
    (*reflectedLight).indirectDiffuse += diffuse * cosineWeightedIrradiance;
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    var dif = vec3<f32>(1.0, 1.0, 1.0);
    var emi = vec3<f32>(0.0, 0.0, 0.0);
    var rou = 0.3;
    var met = 0.0;

    var diffuseColor = vec4<f32>(dif, 1.0);
    var reflectedLight = ReflectedLight(vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));
    let totalEmissiveRadiance = emi;
    diffuseColor *= in.color;
    let roughnessFactor = rou;
    let metalnessFactor = met;
    let faceDirection = select(-1.0, 1.0, in.frontFacing);
    let normal = normalize(in.normal);
    var material: PhysicalMaterial;
    material.diffuseColor = diffuseColor.rgb * (1.0 - metalnessFactor);
    let dxy = max(abs(dpdx(normal)), abs(dpdy(normal)));
    let geometryRoughness = max(max(dxy.x, dxy.y), dxy.z);
    material.roughness = max(roughnessFactor, 0.0525);
    material.roughness += geometryRoughness;
    material.roughness = min(material.roughness, 1.0);
    material.specularColor = mix(vec3<f32>(0.04), diffuseColor.rgb, metalnessFactor);
    material.specularF90 = 1.0;

    var geometry: GeometricContext;
    geometry.position = -in.viewPosition;
    geometry.normal = normal;
    geometry.viewDir = select(in.viewPosition, vec3<f32>(0.0, 0.0, 1.0), projection.isOrthographic == 1);
    var iblIrradiance = vec3<f32>(0.0);
    let irradiance = getAmbientLightIrradiance(ambientLightColor);
    var radiance = vec3<f32>(0.0);
    var clearcoatRadiance = vec3<f32>(0.0);
    iblIrradiance += getIBLIrradiance(geometry.normal);
    radiance += getIBLRadiance(geometry.viewDir, geometry.normal, material.roughness);
    RE_IndirectDiffuse(irradiance, geometry, material, &reflectedLight);
    RE_IndirectSpecular(radiance, iblIrradiance, clearcoatRadiance, geometry, material, &reflectedLight);
    let totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;
    let totalSpecular = reflectedLight.directSpecular + reflectedLight.indirectSpecular;
    let outgoingLight = totalDiffuse + totalSpecular + totalEmissiveRadiance;

    return vec4<f32>(outgoingLight, diffuseColor.a);
}
