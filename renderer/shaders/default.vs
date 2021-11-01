// Portions of shader code has been taken from Three.js source code.
//
// Copyright © 2010-2021 three.js authors

precision mediump float;

uniform mat4 modelMatrix;
uniform mat4 projection;
uniform mat4 modelView;
uniform mat3 normalMatrix;

in vec3 position;
in vec3 normal;

#ifdef USE_INSTANCING
    in mat4 instancedModelMatrix;
    #ifdef USE_INSTANCED_COLORS
        in vec4 instancedColor;
    #else
        uniform vec4 color;
    #endif
#else
    uniform vec4 color;
    
#endif

out vec3 vViewPosition;
out vec3 vNormal;
out vec4 vColor;

void main() {
    vec4 mvPosition = vec4(position, 1.0);
    vec3 transformedNormal = normal;
    #ifdef USE_INSTANCING
        mvPosition = instancedModelMatrix * mvPosition;
        mat3 m = mat3(instancedModelMatrix);
        transformedNormal /= vec3(dot(m[0], m[0]), dot(m[1], m[1]), dot(m[2], m[2]));
        transformedNormal = m * transformedNormal;
        #ifdef USE_INSTANCED_COLORS
            vColor = instancedColor;
        #else
            vColor = color;
        #endif
    #else
        vColor = color;
    #endif 
    vNormal = normalize(normalMatrix * transformedNormal);
    vNormal.y = -vNormal.y;

    mvPosition = modelView * mvPosition;
    gl_Position = projection * mvPosition;
  
    vViewPosition = -mvPosition.xyz;
}
